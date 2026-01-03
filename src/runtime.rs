#[cfg(feature = "blocking")]
use crate::Result;

#[cfg(all(feature = "blocking", feature = "rt-tokio"))]
use crate::Error;

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

pub(crate) type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

#[cfg(feature = "rt-async-io")]
pub(crate) fn sleep(duration: Duration) -> BoxFuture<()> {
    Box::pin(async move {
        let _ = async_io::Timer::after(duration).await;
    })
}

#[cfg(feature = "rt-tokio")]
pub(crate) fn sleep(duration: Duration) -> BoxFuture<()> {
    Box::pin(tokio::time::sleep(duration))
}

#[cfg(feature = "blocking")]
pub(crate) fn block_on_result<T>(future: impl Future<Output = Result<T>>) -> Result<T> {
    #[cfg(feature = "rt-async-io")]
    {
        async_io::block_on(future)
    }

    #[cfg(feature = "rt-tokio")]
    {
        tokio_block_on_result(future)
    }
}

#[cfg(all(feature = "blocking", feature = "rt-tokio"))]
fn tokio_block_on_result<T>(future: impl Future<Output = Result<T>>) -> Result<T> {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => match handle.runtime_flavor() {
            tokio::runtime::RuntimeFlavor::MultiThread => {
                tokio::task::block_in_place(|| handle.block_on(future))
            }
            tokio::runtime::RuntimeFlavor::CurrentThread => Err(Error::invalid_input(
                "blocking API cannot run inside a tokio current-thread runtime; use async API or a multi-thread tokio runtime",
            )),
            _ => Err(Error::invalid_input(
                "blocking API cannot run inside the current tokio runtime",
            )),
        },
        Err(_) => {
            type Init = std::result::Result<tokio::runtime::Runtime, String>;
            static RT: std::sync::OnceLock<Init> = std::sync::OnceLock::new();

            let rt = match RT.get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())
            }) {
                Ok(rt) => rt,
                Err(detail) => {
                    return Err(Error::IoError {
                        context: format!("init tokio runtime: {detail}"),
                    });
                }
            };

            rt.block_on(future)
        }
    }
}
