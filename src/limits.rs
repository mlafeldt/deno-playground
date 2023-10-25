use std::env;
use std::rc::Rc;

use clap::Parser;
use deno_runtime::{
    deno_core::{anyhow::Result, resolve_path, v8, FsModuleLoader},
    permissions::PermissionsContainer,
    worker::{MainWorker, WorkerOptions},
};
use tokio::time::Duration;

#[derive(Parser, Debug)]
struct Args {
    #[arg(default_value = "js/hello.js")]
    module_path: std::path::PathBuf,

    #[arg(short, long, default_value_t = 100, value_name = "MiB")]
    memory_limit: usize,

    #[arg(short, long, default_value_t = 3000, value_name = "ms")]
    timeout: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("Running {}", args.module_path.display());
    let cwd = env::current_dir()?;
    let main_module = resolve_path(args.module_path.to_str().unwrap(), &cwd)?;

    let memory_limit = args.memory_limit * 1024 * 1024;
    let timeout = Duration::from_millis(args.timeout);

    let options = WorkerOptions {
        module_loader: Rc::new(FsModuleLoader),
        create_params: Some(v8::CreateParams::default().heap_limits(0, memory_limit)),
        ..Default::default()
    };
    let mut worker =
        MainWorker::bootstrap_from_options(main_module.clone(), PermissionsContainer::allow_all(), options);

    // Terminate isolate when approaching memory limit
    let isolate_handle = worker.js_runtime.v8_isolate().thread_safe_handle();
    worker
        .js_runtime
        .add_near_heap_limit_callback(move |current_limit, _initial_limit| {
            println!("Terminating isolate near memory limit (current={current_limit} max={memory_limit})");
            isolate_handle.terminate_execution();
            current_limit * 2
        });

    // Terminate isolate after timeout
    let isolate_handle = worker.js_runtime.v8_isolate().thread_safe_handle();
    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(timeout) => {
                println!("Terminating isolate after timeout ({timeout:?})");
                isolate_handle.terminate_execution();
            }
        }
    });

    worker.execute_main_module(&main_module).await?;
    worker.run_event_loop(false).await?;

    Ok(())
}
