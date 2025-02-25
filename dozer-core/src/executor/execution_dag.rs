use std::{
    borrow::BorrowMut,
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
    fmt::Debug,
    rc::Rc,
    sync::Arc,
};

use crate::{
    builder_dag::{BuilderDag, NodeKind, NodeType},
    epoch::EpochManager,
    errors::ExecutionError,
    hash_map_to_vec::insert_vec_element,
    node::PortHandle,
    record_store::{create_record_store, RecordReader, RecordWriter},
};
use crossbeam::channel::{bounded, Receiver, Sender};
use daggy::petgraph::{
    visit::{EdgeRef, IntoEdges, IntoEdgesDirected, IntoNodeIdentifiers},
    Direction,
};

use super::ExecutorOperation;

pub type SharedRecordWriter = Rc<RefCell<Option<Box<dyn RecordWriter>>>>;

#[derive(Debug, Clone)]
pub struct EdgeType {
    /// Output port handle.
    pub output_port: PortHandle,
    /// The sender for data flowing downstream.
    pub sender: Sender<ExecutorOperation>,
    /// The record writer for persisting data for downstream queries, if persistency is needed. Different edges with the same output port share the same record writer.
    pub record_writer: SharedRecordWriter,
    /// Input port handle.
    pub input_port: PortHandle,
    /// The receiver from receiving data from upstream.
    pub receiver: Receiver<ExecutorOperation>,
    /// The record reader for reading persisted data of corresponding output port, if there's some.
    pub record_reader: Option<Box<dyn RecordReader>>,
}

#[derive(Debug)]
pub struct ExecutionDag {
    /// Nodes will be moved into execution threads.
    graph: daggy::Dag<Option<NodeType>, EdgeType>,
    epoch_manager: Arc<EpochManager>,
}

impl ExecutionDag {
    pub fn new(builder_dag: BuilderDag, channel_buffer_sz: usize) -> Result<Self, ExecutionError> {
        // Count number of sources.
        let num_sources = builder_dag
            .graph()
            .node_identifiers()
            .filter(|node_index| {
                matches!(
                    builder_dag.graph()[*node_index].kind,
                    NodeKind::Source(_, _)
                )
            })
            .count();

        // We only create record stored once for every output port. Every `HashMap` in this `Vec` tracks if a node's output ports already have the record store created.
        let mut all_record_stores =
            vec![
                HashMap::<PortHandle, (SharedRecordWriter, Option<Box<dyn RecordReader>>)>::new();
                builder_dag.graph().node_count()
            ];

        // Create new edges.
        let mut edges = vec![];
        for builder_dag_edge in builder_dag.graph().raw_edges().iter() {
            let source_node_index = builder_dag_edge.source();
            let edge = &builder_dag_edge.weight;
            let output_port = builder_dag_edge.weight.output_port;

            // Create or get record store.
            let (record_writer, record_reader) =
                match all_record_stores[source_node_index.index()].entry(output_port) {
                    Entry::Vacant(entry) => {
                        let record_store = create_record_store(
                            &builder_dag.graph()[source_node_index].storage.master_txn,
                            output_port,
                            edge.output_port_type,
                            edge.schema.clone(),
                            channel_buffer_sz + 1,
                        )?;
                        let (record_writer, record_reader) =
                            if let Some((record_writer, record_reader)) = record_store {
                                (
                                    Rc::new(RefCell::new(Some(record_writer))),
                                    Some(record_reader),
                                )
                            } else {
                                (Rc::new(RefCell::new(None)), None)
                            };
                        entry.insert((record_writer, record_reader)).clone()
                    }
                    Entry::Occupied(entry) => entry.get().clone(),
                };

            // Create channel.
            let (sender, receiver) = bounded(channel_buffer_sz);

            // Create edge.
            let edge = EdgeType {
                output_port,
                sender,
                record_writer,
                input_port: edge.input_port,
                receiver,
                record_reader,
            };
            edges.push(Some(edge));
        }

        // Create new graph.
        let graph = builder_dag.into_graph().map_owned(
            |_, node| Some(node),
            |edge_index, _| {
                edges[edge_index.index()]
                    .take()
                    .expect("We created all edges")
            },
        );
        Ok(ExecutionDag {
            graph,
            epoch_manager: Arc::new(EpochManager::new(num_sources)),
        })
    }

    pub fn graph(&self) -> &daggy::Dag<Option<NodeType>, EdgeType> {
        &self.graph
    }

    pub fn node_weight_mut(&mut self, node_index: daggy::NodeIndex) -> &mut Option<NodeType> {
        &mut self.graph[node_index]
    }

    pub fn epoch_manager(&self) -> &Arc<EpochManager> {
        &self.epoch_manager
    }

    #[allow(clippy::type_complexity)]
    pub fn collect_senders_and_record_writers(
        &mut self,
        node_index: daggy::NodeIndex,
    ) -> (
        HashMap<PortHandle, Vec<Sender<ExecutorOperation>>>,
        HashMap<PortHandle, Box<dyn RecordWriter>>,
    ) {
        let edge_indexes = self
            .graph
            .edges(node_index)
            .map(|edge| edge.id())
            .collect::<Vec<_>>();

        let mut senders = HashMap::new();
        let mut record_writers = HashMap::new();
        for edge_index in edge_indexes {
            let edge = self
                .graph
                .edge_weight_mut(edge_index)
                .expect("We don't modify graph structure, only modify the edge weight");
            insert_vec_element(&mut senders, edge.output_port, edge.sender.clone());
            if let Entry::Vacant(entry) = record_writers.entry(edge.output_port) {
                // This interior mutability is to word around `Rc`. Other parts of this function is correctly marked `mut`.
                if let Some(record_writer) = edge.record_writer.borrow_mut().take() {
                    entry.insert(record_writer);
                }
            }
        }

        (senders, record_writers)
    }

    #[allow(clippy::type_complexity)]
    pub fn collect_receivers_and_record_readers(
        &mut self,
        node_index: daggy::NodeIndex,
    ) -> (
        Vec<PortHandle>,
        Vec<Receiver<ExecutorOperation>>,
        HashMap<PortHandle, Box<dyn RecordReader>>,
    ) {
        let edge_indexes = self
            .graph
            .edges_directed(node_index, Direction::Incoming)
            .map(|edge| edge.id())
            .collect::<Vec<_>>();

        let mut input_ports = Vec::new();
        let mut receivers = Vec::new();
        let mut record_readers = HashMap::new();
        for edge_index in edge_indexes {
            let edge = self
                .graph
                .edge_weight_mut(edge_index)
                .expect("We don't modify graph structure, only modify the edge weight");
            input_ports.push(edge.input_port);
            receivers.push(edge.receiver.clone());
            if let Some(record_reader) = edge.record_reader.take() {
                let insert_result = record_readers.insert(edge.input_port, record_reader);
                debug_assert!(
                    insert_result.is_none(),
                    "More than one output connect to a input port"
                );
            }
        }
        (input_ports, receivers, record_readers)
    }
}
