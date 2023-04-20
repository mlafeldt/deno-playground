use std::env;
use std::rc::Rc;

use clap::Parser;
use deno_core::{resolve_path, v8, FsModuleLoader, JsRuntime, RuntimeOptions};
use rocket::{http::Status, launch, post, response::status, routes, State};

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long, default_value_t = 100, value_name = "MiB")]
    memory_limit: usize,

    #[arg(short, long, default_value_t = 3000, value_name = "ms")]
    timeout: u64,
}

#[post("/<name>")]
async fn exec(name: &str, args: &State<Args>) -> status::Custom<String> {
    let cwd = env::current_dir().unwrap();
    let module_spec = resolve_path(&format!("js/{name}.js"), &cwd).unwrap();

    let memory_limit = args.memory_limit * 1024 * 1024;
    let _timeout = tokio::time::Duration::from_millis(args.timeout);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let res = std::thread::spawn(move || {
        let local = tokio::task::LocalSet::new();

        local.block_on(&rt, async move {
            let mut runtime = JsRuntime::new(RuntimeOptions {
                module_loader: Some(Rc::new(FsModuleLoader)),
                create_params: Some(v8::CreateParams::default().heap_limits(0, memory_limit)),
                ..Default::default()
            });

            // Terminate isolate when approaching memory limit
            let isolate_handle = runtime.v8_isolate().thread_safe_handle();
            runtime.add_near_heap_limit_callback(move |current_limit, _initial_limit| {
                println!("Terminating isolate near memory limit (current={current_limit} max={memory_limit})");
                isolate_handle.terminate_execution();
                current_limit * 2
            });

            // TODO: Terminate isolate after timeout

            let module_id = runtime.load_main_module(&module_spec, None).await.unwrap();
            let mut receiver = runtime.mod_evaluate(module_id);

            tokio::select! {
              biased;

              maybe_result = &mut receiver => {
                println!("Result = {maybe_result:#?}");
                maybe_result.expect("Module evaluation result not provided.")
              }

              event_loop_result = runtime.run_event_loop(false) => {
                event_loop_result?;
                let maybe_result = receiver.await;
                maybe_result.expect("Module evaluation result not provided.")
              }
            }
        })
    })
    .join()
    .unwrap();

    match res {
        Ok(_) => status::Custom(Status::Ok, "OK".to_string()),
        Err(e) => status::Custom(Status::InternalServerError, e.to_string()),
    }
}

#[launch]
fn rocket() -> _ {
    rocket::build().manage(Args::parse()).mount("/", routes![exec])
}
