use crate::output_manager_service::storage::database::OutputManagerBackend;


use crate::assets::AssetManagerHandle;
use crate::assets::infrastructure::AssetManagerService;
use log::*;

use futures::{future, Future};



use tari_service_framework::{
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
    ServiceInitializerContext,
};

use crate::output_manager_service::handle::OutputManagerHandle;


const LOG_TARGET: &str = "wallet::assets::infrastructure::initializer";

pub struct AssetManagerServiceInitializer<T>
    where T: OutputManagerBackend
{
    backend: Option<T>
}

impl<T> AssetManagerServiceInitializer<T>
    where T: OutputManagerBackend + 'static
{
    pub fn new(backend: T

    ) -> Self {
        Self {
            backend: Some(backend)
        }
    }
}

impl<T> ServiceInitializer for AssetManagerServiceInitializer<T>
    where T: OutputManagerBackend + 'static
{
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(&mut self, context: ServiceInitializerContext) -> Self::Future {

        let (sender, receiver) = reply_channel::unbounded();

        let handle = AssetManagerHandle::new(sender);
        context.register_handle(handle);

        let backend = self.backend.take().expect("this expect pattern is dumb");

        context.spawn_when_ready(move |handles| async move {

            let output_manager  = handles.expect_handle::<OutputManagerHandle>();
            // let transaction_service = handles.expect_handle::<TransactionServiceHandle>();
            let service = AssetManagerService::new(backend, output_manager);

            let running = service.start(handles.get_shutdown_signal(), receiver);

            futures::pin_mut!(running);
            future::select(running, handles.get_shutdown_signal()).await;
            info!(target: LOG_TARGET, "Asset Manager Service shutdown");
        });
        future::ready(Ok(()))
    }
}

