async fn old_main() -> std::io::Result<()> {
    // let flags = xflags::parse_or_exit! {
    //     /// Port to serve on
    //     required port: u16
    // };

    println!("Starting.");
    // let manager = WorkerManager::new();

    // let worker_settings = WorkerSettings {
    //     log_level: WorkerLogLevel::Debug,
    //     log_tags: vec![],
    //     rtc_ports_range: todo!(),
    //     dtls_files: todo!(),
    //     thread_initializer: todo!(),
    //     app_data: todo!(),
    // };
    // let worker = manager.create_worker(worker_settings);

    // GET /hello/warp => 200 OK with body "Hello, warp!"
    // let hello = warp::path!("hello" / String).map(|name| format!("Hello, {}!\n", name));

    // println!("Serving on port {}.", &flags.port);
    // warp::serve(hello).run(([0, 0, 0, 0], flags.port)).await;
    println!("Done.");

    Ok(())
}
