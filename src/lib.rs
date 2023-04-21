use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::task::JoinError;
use tokio::time::sleep;

#[cfg(feature = "tracing")]
use tracing::{error, info, warn};

pub type RetryResult<T> = Result<T, JoinError>;

pub enum RetryPolicy<T> {
    Quit(T),
    Repeat,
    WaitRepeat(Duration),
}

pub trait ErrorHandler<T, Q> {
    fn handle(&mut self, result: Result<T, JoinError>) -> RetryPolicy<Q>;
}

pub trait FutureFactory<T>
where
    T: 'static,
{
    fn build_future(&mut self) -> Pin<Box<dyn Future<Output = T> + Send>>;
}

pub async fn make_future_respawnable<T, Q, F>(mut handler: impl ErrorHandler<T, Q>, mut f: F) -> Q
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

impl<T> ErrorHandler<T, ()> for AlwaysRespawn {
    fn handle(&mut self, _result: Result<T, JoinError>) -> RetryPolicy<()> {
        RetryPolicy::WaitRepeat(self.duration)
    }
}

#[cfg(feature = "tracing")]
pub struct AlwaysRespawnAndTraceResults {
    duration: std::time::Duration,
}

#[cfg(feature = "tracing")]
pub struct AlwaysRespawnAndTrace {
    duration: std::time::Duration,
}

#[cfg(feature = "tracing")]
impl<T> ErrorHandler<T, ()> for AlwaysRespawnAndTrace {
    fn handle(&mut self, result: Result<T, tokio::task::JoinError>) -> RetryPolicy<()> {
        match result {
            Ok(_ok) => warn!("task finished"),
            Err(err) => error!(?err, "task failed"),
        }
        RetryPolicy::WaitRepeat(self.duration)
    }
}

#[cfg(feature = "tracing")]
impl<T: Debug, E: std::error::Error> ErrorHandler<Result<T, E>, ()>
    for AlwaysRespawnAndTraceResults
{
    fn handle(&mut self, result: Result<Result<T, E>, JoinError>) -> RetryPolicy<()> {
        match result {
            Ok(finished) => match finished {
                Ok(ok) => {
                    info!(?ok, "task completed");
                }
                Err(err) => {
                    error!(?err, "task completed with error");
                }
            },
            Err(join) => {
                if join.is_panic() {
                    error!(?join, "task aborted with panic");
                } else {
                    error!(?join, "task aborted with error");
                }
            }
        }
        RetryPolicy::WaitRepeat(self.duration)
    }
}

pub struct RetriesTimes {
    duration: std::time::Duration,
    count: usize,
}

impl<T> ErrorHandler<T, Result<T, JoinError>> for RetriesTimes {
    fn handle(&mut self, result: Result<T, JoinError>) -> RetryPolicy<Result<T, JoinError>> {
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

    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::{io, panic};

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
        let count = Arc::new(AtomicU32::new(0));
        let takes_dummy = TakesDummy {
            dummy: Dummy,
            count: count.clone(),
        };

        let handler = RetriesTimes {
            duration: std::time::Duration::from_millis(1),
            count: 3,
        };

        let join_handle = tokio::spawn(make_future_respawnable::<(), _, _>(handler, takes_dummy));

        let err = join_handle.await.unwrap().unwrap_err();
        assert!(err.is_panic());
        assert_eq!(count.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn check_always_respawn_compiles() {
        let count = Arc::new(AtomicU32::new(0));
        let takes_dummy = TakesDummy {
            dummy: Dummy,
            count: count.clone(),
        };

        let handler = AlwaysRespawn {
            duration: std::time::Duration::from_millis(1),
        };

        let join_handle = tokio::spawn(make_future_respawnable::<(), _, _>(handler, takes_dummy));

        sleep(Duration::from_millis(10)).await;

        join_handle.abort();

        let err = join_handle.await.unwrap_err();
        assert!(!err.is_panic(), "{:?}", err);
        assert!(err.is_cancelled(), "{:?}", err);
    }

    struct TaskReturnsResult;

    impl FutureFactory<Result<(), io::Error>> for TaskReturnsResult {
        fn build_future(&mut self) -> Pin<Box<dyn Future<Output = Result<(), io::Error>> + Send>> {
            Box::pin(async { panic!("boom") })
        }
    }

    #[cfg(feature = "tracing")]
    #[tokio::test]
    async fn check_always_respawn_and_trace_result_compiles() {
        let factory = TaskReturnsResult;

        let handler = AlwaysRespawnAndTraceResults {
            duration: std::time::Duration::from_millis(1),
        };

        let join_handle = tokio::spawn(make_future_respawnable::<_, _, _>(handler, factory));

        sleep(Duration::from_millis(10)).await;

        join_handle.abort();

        let err = join_handle.await.unwrap_err();
        assert!(!err.is_panic(), "{:?}", err);
        assert!(err.is_cancelled(), "{:?}", err);
    }

    struct TaskReturnsVoid;

    impl FutureFactory<()> for TaskReturnsVoid {
        fn build_future(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send>> {
            Box::pin(async { panic!("boom") })
        }
    }

    #[cfg(feature = "tracing")]
    #[tokio::test]
    async fn check_always_respawn_and_trace_compiles() {
        let factory = TaskReturnsVoid;

        let handler = AlwaysRespawnAndTrace {
            duration: std::time::Duration::from_millis(1),
        };

        let join_handle = tokio::spawn(make_future_respawnable::<_, _, _>(handler, factory));

        sleep(Duration::from_millis(10)).await;

        join_handle.abort();

        let err = join_handle.await.unwrap_err();
        assert!(!err.is_panic(), "{:?}", err);
        assert!(err.is_cancelled(), "{:?}", err);
    }
}
