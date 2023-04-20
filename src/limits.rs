use std::env;
use std::rc::Rc;

use clap::Parser;
use deno_core::{anyhow::Result, resolve_path, v8, FsModuleLoader, JsRuntime, RuntimeOptions};
use tokio::time;

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
    let module_spec = resolve_path(args.module_path.to_str().unwrap(), &cwd)?;

    let memory_limit = args.memory_limit * 1024 * 1024;
    let timeout = time::Duration::from_millis(args.timeout);

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

    // Terminate isolate after timeout
    let isolate_handle = runtime.v8_isolate().thread_safe_handle();
    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(timeout) => {
                println!("Terminating isolate after timeout ({timeout:?})");
                isolate_handle.terminate_execution();
            }
        }
    });

    let module_id = runtime.load_main_module(&module_spec, None).await?;
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
}
