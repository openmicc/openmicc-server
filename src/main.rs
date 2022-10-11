use warp::Filter;

#[tokio::main]
async fn main() {
    let flags = xflags::parse_or_exit! {
        /// Port to serve on
        required port: u16
    };

    println!("Starting.");
    // GET /hello/warp => 200 OK with body "Hello, warp!"
    let hello = warp::path!("hello" / String).map(|name| format!("Hello, {}!\n", name));

    println!("Serving on port {}.", &flags.port);
    warp::serve(hello).run(([0, 0, 0, 0], flags.port)).await;
    println!("Done.");
}
