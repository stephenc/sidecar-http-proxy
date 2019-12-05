// Copyright 2019 Stephen Connolly.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE.txt or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT.txt or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::env;
use std::str::FromStr;

use chrono::Utc;
use futures::future::{self, Future};
use getopts::Options;
use hyper::header::HeaderValue;
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, HeaderMap, Request, Response, Server, Uri};

type BoxFut = Box<dyn Future<Item = Response<Body>, Error = hyper::Error> + Send>;

fn debug_request(req: Request<Body>) -> BoxFut {
    let body_str = format!("{:?}", req);
    let response = Response::builder()
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Body::from(body_str))
        .unwrap();
    Box::new(future::ok(response))
}

fn redirect(url: &str) -> BoxFut {
    let response = Response::builder()
        .status(301)
        .header("Location", url)
        .body(Body::empty())
        .unwrap();
    Box::new(future::ok(response))
}

fn not_found() -> BoxFut {
    let response = Response::builder()
        .status(404)
        .body(Body::from(
            "
            <!DOCTYPE html>
            <html>
            <head>
            <title>Not Found</title>
            <style>
                body {
                    width: 35em;
                    margin: 0 auto;
                    font-family: Tahoma, Verdana, Arial, sans-serif;
                }
            </style>
            </head>
            <body>
            <h1>Not Found</h1>
            <p>Sorry, the page you are looking for cannot be found</p>
            </body>
            </html>
        ",
        ))
        .unwrap();
    Box::new(future::ok(response))
}

fn create_options() -> Options {
    let mut opts = Options::new();
    opts.optflag("h", "help", "print this help menu and exit");
    opts.optflag("V", "version", "print the version and exit");
    opts.optopt(
        "p",
        "port",
        "the port to listen for requests on (default: 8080)",
        "PORT",
    );
    opts.optopt("t", "target-url", "the target base URL to proxy", "URL");
    opts.optopt(
        "s",
        "source-path",
        "the source path to remove from requests before forwarding to the target (default: /)",
        "PATH",
    );
    opts.optopt(
        "c",
        "cache-control",
        "the cache control header to inject if none is provided",
        "VALUE",
    );
    opts
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options]", program);
    println!("{}", opts.usage(&brief));
    println!();
    println!("Proxies requests to a remote service (with optional path prefix stripping)");
}

fn main() {
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let opts = create_options();
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => panic!(f.to_string()),
    };
    if matches.opt_present("h") {
        print_usage(&program, opts);
        return;
    }
    if matches.opt_present("V") {
        println!("{}", VERSION);
        return;
    }

    let port = match matches.opt_str("p") {
        Some(v) => match v.parse::<u16>() {
            Ok(v) => v,
            Err(_) => panic!("Port is supposed to be an integer in the range 0-65535"),
        },
        None => 8080,
    };

    let target: Box<String> = Box::from(match matches.opt_str("t") {
        Some(v) => v.trim_end_matches('/').to_string(),
        None => panic!("You must provide the target URL"),
    });

    let source: Box<String> = Box::from(match matches.opt_str("s") {
        Some(v) => v.trim_matches('/').to_string(),
        None => "".to_string(),
    });

    let cache: Option<Box<String>> = match matches.opt_str("c") {
        Some(v) => Option::Some(Box::from(v)),
        None => Option::None,
    };

    // This is our socket address...
    let addr = ([0, 0, 0, 0], port).into();

    // A `Service` is needed for every connection.
    let make_svc = make_service_fn(move |socket: &AddrStream| {
        let target_url = target.clone();
        let source_match = format!("/{}", source.clone());
        let source_prefix = format!("/{}/", source.clone());
        let remote_addr = socket.remote_addr();
        let cache_control = match &cache {
            Some(v) => Some(v.clone()),
            _ => None,
        };
        service_fn(move |mut req: Request<Body>| {
            // returns BoxFut
            if req.uri().path().starts_with(source_prefix.as_str()) {
                let request_uri = format!("{}", req.uri());
                let forward_uri = match req.uri().query() {
                    Some(query) => format!(
                        "{}?{}",
                        req.uri().path().replace(source_prefix.as_str(), "/"),
                        query
                    ),
                    None => format!("{}", req.uri().path().replace(source_prefix.as_str(), "/")),
                };
                println!(
                    "[{}] {} Proxy {}{}",
                    Utc::now(),
                    request_uri,
                    target_url,
                    forward_uri
                );
                *req.uri_mut() = Uri::from_str(forward_uri.as_str()).unwrap();
                let future = hyper_reverse_proxy::call(remote_addr.ip(), target_url.as_str(), req);
                match &cache_control {
                    Some(value) => {
                        let header_value = HeaderValue::from_str(value.clone().as_str()).unwrap();
                        Box::new(future.map(|mut r| {
                            if !r.headers().contains_key("Cache-Control") {
                                let mut headers = HeaderMap::new();
                                for (k, v) in r.headers().iter() {
                                    headers.insert(k.clone(), v.clone());
                                }
                                headers.insert("Cache-Control", header_value);
                                *r.headers_mut() = headers;
                            }
                            r
                        }))
                    }
                    _ => future,
                }
            } else if req.uri().path().eq(source_match.as_str()) {
                println!(
                    "[{}] {} HTTP/301 Location: {}",
                    Utc::now(),
                    req.uri(),
                    source_prefix
                );
                redirect(source_prefix.as_str())
            } else {
                if req.headers().contains_key("X-Proxy-Debug") {
                    println!("[{}] {} Debug {:?}", Utc::now(), req.uri(), req);
                    debug_request(req)
                } else {
                    println!("[{}] {} HTTP/404", Utc::now(), req.uri());
                    not_found()
                }
            }
        })
    });

    let server = Server::bind(&addr)
        .serve(make_svc)
        .map_err(|e| eprintln!("server error: {}", e));

    println!("Running server on {:?}", addr);

    // Run this server for... forever!
    hyper::rt::run(server);
}
