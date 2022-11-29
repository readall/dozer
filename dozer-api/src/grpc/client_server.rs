use super::{
    common::CommonService,
    common_grpc::common_grpc_service_server::CommonGrpcServiceServer,
    dynamic::util::{create_descriptor_set, read_file_as_byte},
    internal_grpc::{pipeline_request::ApiEvent, PipelineRequest},
    types::SchemaEvent,
};
use crate::{
    errors::GRPCError, generator::protoc::generator::ProtoGenerator, grpc::dynamic::DynamicService,
    CacheEndpoint, PipelineDetails,
};
use futures_util::FutureExt;
use heck::ToUpperCamelCase;
use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    thread,
};
use tokio::sync::broadcast;
use tonic::transport::Server;
use tonic_reflection::server::{ServerReflection, ServerReflectionServer};

pub struct ApiServer {
    port: u16,
    dynamic: bool,
    event_notifier: crossbeam::channel::Receiver<PipelineRequest>,
}

impl ApiServer {
    pub fn new(
        event_notifier: crossbeam::channel::Receiver<PipelineRequest>,
        port: u16,
        dynamic: bool,
    ) -> Self {
        Self {
            port,
            event_notifier,
            dynamic,
        }
    }
    pub fn setup_broad_cast_channel(
        sender: broadcast::Sender<PipelineRequest>,
        event_notifier: crossbeam::channel::Receiver<PipelineRequest>,
    ) -> Result<(), GRPCError> {
        let _thread = thread::spawn(move || {
            while let Some(event) = event_notifier.iter().next() {
                _ = sender.send(event);
            }
        });
        Ok(())
    }
    fn get_dynamic_service(
        &self,
        pipeline_map: HashMap<String, PipelineDetails>,
        rx1: broadcast::Receiver<PipelineRequest>,
        _running: Arc<AtomicBool>,
    ) -> Result<
        (
            DynamicService,
            ServerReflectionServer<impl ServerReflection>,
        ),
        GRPCError,
    > {
        let mut schemas: HashMap<String, SchemaEvent> = HashMap::new();
        // wait until all schemas are initalized
        while schemas.len() < pipeline_map.len() {
            if let Ok(event) = self.event_notifier.recv() {
                let api_event = event.api_event;
                if let Some(ApiEvent::Schema(schema)) = api_event {
                    schemas.insert(event.endpoint, schema);
                }
            }
        }
        let tmp_dir = Path::new("./.dozer").join("proto_generated");
        if tmp_dir.exists() {
            fs::remove_dir_all(&tmp_dir).unwrap();
        }
        fs::create_dir_all(&tmp_dir).unwrap();

        let tempdir_path = String::from(tmp_dir.to_str().unwrap());

        let proto_generator = ProtoGenerator::new(pipeline_map.to_owned())?;
        let generated_proto = proto_generator.generate_proto(tempdir_path.to_owned())?;

        let descriptor_path = create_descriptor_set(&tempdir_path, "generated.proto")
            .map_err(|e| GRPCError::InternalError(Box::new(e)))?;

        let vec_byte = read_file_as_byte(descriptor_path.to_owned())
            .map_err(|e| GRPCError::InternalError(Box::new(e)))?;

        let inflection_service = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(vec_byte.as_slice())
            .build()?;
        let mut pipeline_hashmap: HashMap<String, PipelineDetails> = HashMap::new();
        for (_, pipeline_details) in pipeline_map.iter() {
            pipeline_hashmap.insert(
                format!(
                    "Dozer.{}Service",
                    pipeline_details.schema_name.to_upper_camel_case()
                ),
                pipeline_details.to_owned(),
            );
        }

        // Service handling dynamic gRPC requests.
        let grpc_service = DynamicService::new(
            descriptor_path,
            generated_proto.1,
            pipeline_hashmap,
            rx1.resubscribe(),
        );
        Ok((grpc_service, inflection_service))
    }

    pub async fn run(
        &self,
        cache_endpoints: Vec<CacheEndpoint>,
        running: Arc<AtomicBool>,
        receiver_shutdown: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<(), GRPCError> {
        // create broadcast channel
        let (tx, rx1) = broadcast::channel::<PipelineRequest>(16);
        let mut pipeline_map: HashMap<String, PipelineDetails> = HashMap::new();

        for ce in cache_endpoints {
            pipeline_map.insert(
                ce.endpoint.name.to_owned(),
                PipelineDetails {
                    schema_name: ce.endpoint.name.to_owned(),
                    cache_endpoint: ce.to_owned(),
                },
            );
        }

        let common_service = CommonGrpcServiceServer::new(CommonService {
            pipeline_map: pipeline_map.to_owned(),
            event_notifier: rx1.resubscribe(),
        });

        let grpc_router = Server::builder()
            .accept_http1(true)
            .concurrency_limit_per_connection(32)
            .add_service(
                tonic_web::config()
                    .allow_all_origins()
                    .enable(common_service),
            );

        let grpc_router = if self.dynamic {
            let (grpc_service, inflection_service) =
                self.get_dynamic_service(pipeline_map.clone(), rx1, running)?;
            // GRPC service to handle reflection requests
            grpc_router
                .add_service(inflection_service)
                // GRPC service to handle typed requests
                .add_service(tonic_web::config().allow_all_origins().enable(grpc_service))
        } else {
            grpc_router
        };
        ApiServer::setup_broad_cast_channel(tx, self.event_notifier.to_owned())?;
        let addr = format!("[::0]:{:}", self.port).parse().unwrap();
        grpc_router
            .serve_with_shutdown(addr, receiver_shutdown.map(drop))
            .await
            .map_err(|e| GRPCError::InternalError(Box::new(e)))
    }
}
