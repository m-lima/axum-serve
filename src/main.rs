mod args;

fn main() -> impl std::process::Termination {
    let args = args::parse();

    if let Err(e) = axum_boiler::log::setup(args.verbosity) {
        eprintln!("{e}");
        return e.into();
    }

    tracing::info!(?args, "Configuration loaded");

    for (port, serve_points) in args.serve_points {
        let mut router = axum::Router::<(), hyper::Body>::new();
        for serve_point in serve_points {
            match serve_point.target {
                args::Target::Dir(dir) => {
                    router = router.nest_service(
                        &serve_point.path,
                        axum::routing::get_service(tower_http::services::ServeDir::new(dir))
                            .handle_error(|_| async {
                                status_response(hyper::StatusCode::NOT_FOUND)
                            }),
                    );
                }
                args::Target::Net(net) => {
                    router = router.nest_service(
                        &serve_point.path,
                        Proxy {
                            client: hyper::Client::new(),
                            target: net,
                        },
                    );
                }
            }
        }

        if args.cors {
            router = router.layer(tower_http::cors::CorsLayer::very_permissive());
        }

        if let Err(e) = axum_boiler::serve(router, ([0, 0, 0, 0], port)) {
            tracing::error!("{e}");
            return e.into();
        }
    }

    std::process::ExitCode::SUCCESS
}

#[derive(Clone, Debug)]
struct Proxy {
    client: hyper::Client<hyper::client::HttpConnector, hyper::Body>,
    target: hyper::Uri,
}

impl tower::Service<hyper::Request<hyper::Body>> for Proxy {
    type Response = hyper::Response<hyper::Body>;
    type Error = std::convert::Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    #[tracing::instrument(fields(yo = "bla"), skip_all)]
    fn call(&mut self, request: hyper::Request<hyper::Body>) -> Self::Future {
        let client = self.client.clone();

        let uri = format!(
            "{target}{path}",
            target = self.target,
            path = request
                .uri()
                .path_and_query()
                .map_or(request.uri().path(), |v| v.as_str())
        );

        Box::pin(proxy(client, uri, request))
    }
}

#[tracing::instrument(fields(%uri), skip_all)]
async fn proxy(
    client: hyper::Client<hyper::client::HttpConnector, hyper::Body>,
    uri: String,
    mut request: hyper::Request<hyper::Body>,
) -> Result<hyper::Response<hyper::Body>, std::convert::Infallible> {
    match hyper::Uri::try_from(&uri) {
        Ok(uri) => {
            *request.uri_mut() = uri;
            client.request(request).await.or_else(|e| {
                tracing::error!("Proxy error: {e}");
                Ok(status_response(hyper::StatusCode::BAD_GATEWAY))
            })
        }
        Err(e) => {
            tracing::error!("Bad URI `{uri}`: {e}");
            Ok(status_response(hyper::StatusCode::INTERNAL_SERVER_ERROR))
        }
    }
}

fn status_response(status_code: hyper::StatusCode) -> hyper::Response<hyper::Body> {
    let mut response = hyper::Response::new(hyper::Body::empty());
    *response.status_mut() = status_code;
    response
}
