use futures_util::{future::poll_fn, pin_mut};
use std::future::Future;
use tokio::runtime::Handle;
use tokio_test::task;
use tower::Service;
use tower_filter::{error::Error, Filter};
use tower_test::{assert_request_eq, mock};

#[tokio_macros::test]
async fn passthrough_sync() {
    let (mut service, handle) = new_service(|_| async { Ok(()) });

    let handle = Handle::current().spawn(async move {
        // Receive the requests and respond
        pin_mut!(handle);
        for i in 0..10 {
            assert_request_eq!(handle, format!("ping-{}", i)).send_response(format!("pong-{}", i));
        }
    });

    let mut responses = vec![];

    for i in 0usize..10 {
        let request = format!("ping-{}", i);
        poll_fn(|cx| service.poll_ready(cx)).await.unwrap();
        let exchange = service.call(request);
        let exchange = async move {
            let response = exchange.await.unwrap();
            let expect = format!("pong-{}", i);
            assert_eq!(response.as_str(), expect.as_str());
        };

        responses.push(exchange);
    }

    futures_util::future::join_all(responses).await;
    handle.await.unwrap();
}

#[test]
fn rejected_sync() {
    task::spawn(async {
        let (mut service, _handle) = new_service(|_| async { Err(Error::rejected()) });
        service.call("hello".into()).await.unwrap_err();
    });
}

type Mock = mock::Mock<String, String>;
type MockHandle = mock::Handle<String, String>;

fn new_service<F, U>(f: F) -> (Filter<Mock, F>, MockHandle)
where
    F: Fn(&String) -> U,
    U: Future<Output = Result<(), Error>>,
{
    let (service, handle) = mock::pair();
    let service = Filter::new(service, f);
    (service, handle)
}
