// #![deny(warnings)]

use warp::Filter;
use warp::reply::Reply;
use warp::reject::Rejection;
use warp::reply::Response;

macro_rules! with_tracing {
    ($filter:expr) => {
        warp::any()
            .and(warp::filters::method::method())
            .and(warp::filters::path::full())
            .map(|method, path| {
                tracing::info_span!(
                    target: "warp",
                    "request",
                    method = %method,
                    path = ?path,
            // version = ?route.version(),
                );
            })
        .untuple_one()
        .and($filter)
        .then(|result: Result<_, Rejection>| async {
            let resp = result.into_response();
            tracing::debug!(response.status = resp.status().as_u16());
            Ok::<Response, std::convert::Infallible>(resp)
        })
    }
}

#[tokio::main]
async fn main() {
    let filter = std::env::var("RUST_LOG")
        .unwrap_or_else(|_| "tracing=info,warp=debug".to_owned());
    tracing_subscriber::fmt::Subscriber::builder()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    let hello = with_tracing!(warp::path("hello")
        .and(warp::get())
        .map(|| {
            tracing::info!("saying hello...");
            "Hello, World!"
        }));
        // .with(warp::trace::context("hello"));

    let goodbye = with_tracing!(warp::path("goodbye")
        .and(warp::get())
        .and_then(|| async {
            tracing::info!("saying goodbye...");
            Ok::<&'static str, Rejection>("So long and thanks for all the fish!")
        }));
        // .with(warp::trace::context("goodbye"));

    let routes = hello.or(goodbye);
        // .with(warp::trace::request());

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

