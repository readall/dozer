use crate::chk;
use crate::{Dag, Endpoint, DEFAULT_PORT_HANDLE};

use crate::executor::{DagExecutor, ExecutorOptions};
use crate::tests::dag_base_run::NoopJoinProcessorFactory;
use crate::tests::sinks::{CountingSinkFactory, COUNTING_SINK_INPUT_PORT};
use crate::tests::sources::{GeneratorSourceFactory, GENERATOR_SOURCE_OUTPUT_PORT};
use dozer_types::node::NodeHandle;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tempdir::TempDir;

#[test]
fn test_checkpoint_consistency_ns() {
    // dozer_tracing::init_telemetry(false).unwrap();

    const MESSAGES_COUNT: u64 = 25_000;

    let mut dag = Dag::new();

    let sources: Vec<NodeHandle> = vec![
        NodeHandle::new(None, "src1".to_string()),
        NodeHandle::new(None, "src2".to_string()),
        NodeHandle::new(None, "src3".to_string()),
        NodeHandle::new(None, "src4".to_string()),
        NodeHandle::new(None, "src5".to_string()),
    ];

    let latch = Arc::new(AtomicBool::new(true));

    for src_handle in &sources {
        dag.add_source(
            src_handle.clone(),
            Arc::new(GeneratorSourceFactory::new(
                MESSAGES_COUNT,
                latch.clone(),
                true,
            )),
        );
    }

    // Create sources.len()-1 sub dags
    for i in 0..sources.len() - 1 {
        let mut child_dag = Dag::new();

        let proc_handle = NodeHandle::new(None, "proc".to_string());
        let sink_handle = NodeHandle::new(None, "sink".to_string());

        child_dag.add_processor(proc_handle.clone(), Arc::new(NoopJoinProcessorFactory {}));
        child_dag.add_sink(
            sink_handle.clone(),
            Arc::new(CountingSinkFactory::new(MESSAGES_COUNT * 2, latch.clone())),
        );
        chk!(child_dag.connect(
            Endpoint::new(proc_handle, DEFAULT_PORT_HANDLE),
            Endpoint::new(sink_handle, COUNTING_SINK_INPUT_PORT),
        ));

        // Merge the DAG with the parent dag
        dag.merge(Some(i as u16), child_dag);

        chk!(dag.connect(
            Endpoint::new(sources[i].clone(), GENERATOR_SOURCE_OUTPUT_PORT),
            Endpoint::new(NodeHandle::new(Some(i as u16), "proc".to_string()), 1),
        ));
        chk!(dag.connect(
            Endpoint::new(sources[i + 1].clone(), GENERATOR_SOURCE_OUTPUT_PORT),
            Endpoint::new(NodeHandle::new(Some(i as u16), "proc".to_string()), 2),
        ));
    }

    let tmp_dir = chk!(TempDir::new("test"));
    DagExecutor::new(
        dag,
        tmp_dir.path().to_path_buf(),
        ExecutorOptions::default(),
    )
    .unwrap()
    .start(Arc::new(AtomicBool::new(true)))
    .unwrap()
    .join()
    .unwrap();
}
