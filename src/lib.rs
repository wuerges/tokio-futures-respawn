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

pub trait FutureFactory {
    fn build_future<T>(&self) -> Pin<Box<dyn Future<Output = T> + Send>>
    where
        T: 'static;
}

pub async fn make_future_respawnable<T, F>(
    mut handler: impl ErrorHandler<T>,
    f: F,
) -> RetryResult<T>
where
    T: Send + 'static + Debug,
    F: FutureFactory,
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
    use super::*;
    use std::io;
    use std::panic;

    #[derive(Clone, Debug)]
    struct Dummy;

    struct TakesDummy {
        dummy: Dummy,
    }

    impl FutureFactory for TakesDummy {
        fn build_future<T>(&self) -> Pin<Box<dyn Future<Output = T> + Send>> {
            Box::pin(async move { panic!("boom") })
        }
    }

    #[tokio::test]
    async fn test_logging() {
        let dummy = Dummy;
        let takes_dummy = TakesDummy { dummy };

        let handler = RetriesTimes {
            duration: std::time::Duration::from_millis(10),
            count: 3,
        };

        let join_handle = tokio::spawn(make_future_respawnable::<(), _>(handler, takes_dummy));

        join_handle.await.unwrap().unwrap()
    }
}
