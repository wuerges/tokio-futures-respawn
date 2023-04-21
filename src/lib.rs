use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::task::JoinError;
use tokio::time::sleep;

pub type RetryResult<T> = Result<T, JoinError>;

pub enum RetryPolicy<T> {
    Quit(Result<T, JoinError>),
    Repeat,
    WaitRepeat(Duration),
}

pub trait ErrorHandler<T> {
    fn handle(&mut self, result: Result<T, JoinError>) -> RetryPolicy<T>;
}

pub trait FutureFactory<T>
where
    T: 'static,
{
    fn build_future(&mut self) -> Pin<Box<dyn Future<Output = T> + Send>>;
}

pub async fn make_future_respawnable<T, F>(
    mut handler: impl ErrorHandler<T>,
    mut f: F,
) -> RetryResult<T>
where
    T: Send + 'static + Debug,
    F: FutureFactory<T>,
{
    loop {
        let fut = f.build_future();
        let join_handle = tokio::spawn(fut);
        let result = join_handle.await;
        let policy = handler.handle(result);
        match policy {
            RetryPolicy::Repeat => (),
            RetryPolicy::WaitRepeat(duration) => sleep(duration).await,
            RetryPolicy::Quit(x) => return x,
        }
    }
}

pub struct AlwaysRespawn {
    duration: std::time::Duration,
}

impl<T> ErrorHandler<T> for AlwaysRespawn {
    fn handle(&mut self, _result: Result<T, JoinError>) -> RetryPolicy<T> {
        RetryPolicy::WaitRepeat(self.duration)
    }
}

pub struct RetriesTimes {
    duration: std::time::Duration,
    count: usize,
}

impl<T> ErrorHandler<T> for RetriesTimes {
    fn handle(&mut self, result: Result<T, JoinError>) -> RetryPolicy<T> {
        if self.count == 0 {
            RetryPolicy::Quit(result)
        } else {
            self.count -= 1;
            RetryPolicy::WaitRepeat(self.duration)
        }
    }
}

#[cfg(test)]
mod tests {
    use tracing::info;
    use tracing_subscriber::FmtSubscriber;

    use super::*;
    use std::panic;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[derive(Clone, Debug)]
    struct Dummy;

    struct TakesDummy {
        dummy: Dummy,
        count: Arc<AtomicU32>,
    }

    impl FutureFactory<()> for TakesDummy {
        fn build_future(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send>> {
            info!("uses dummy for something {:?}", self.dummy);
            let x = self.count.fetch_add(1, Ordering::Relaxed);
            info!("count: {}", x);
            Box::pin(async move { panic!("boom") })
        }
    }

    #[tokio::test]
    async fn tests_retries_3_times() {
        let subscriber = FmtSubscriber::builder()
            .with_max_level(tracing::Level::TRACE)
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");

        let dummy = Dummy;
        let count = Arc::new(AtomicU32::new(0));
        let takes_dummy = TakesDummy {
            dummy,
            count: count.clone(),
        };

        let handler = RetriesTimes {
            duration: std::time::Duration::from_millis(10),
            count: 3,
        };

        let join_handle = tokio::spawn(make_future_respawnable::<(), _>(handler, takes_dummy));

        let err = join_handle.await.unwrap().unwrap_err();
        assert!(err.is_panic());
        assert_eq!(count.load(Ordering::SeqCst), 4);
    }
}
