use std::env;
use std::rc::Rc;
use std::thread;

use clap::Parser;
use deno_core::{anyhow::Result, resolve_path, v8, FsModuleLoader, JsRuntime, ModuleSpecifier, RuntimeOptions};
use rocket::{http::Status, launch, post, response::status, routes, State};
use tokio::time::Duration;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long, default_value_t = 100, value_name = "MiB")]
    memory_limit: usize,

    #[arg(short, long, default_value_t = 3000, value_name = "ms")]
    timeout: u64,
}

struct Runner {
    runtime: JsRuntime,
    opts: RunnerOpts,
}

struct RunnerOpts {
    memory_limit: usize,
    timeout: Duration,
}

impl Runner {
    fn new(opts: RunnerOpts) -> Self {
        let runtime = JsRuntime::new(RuntimeOptions {
            module_loader: Some(Rc::new(FsModuleLoader)),
            create_params: Some(v8::CreateParams::default().heap_limits(0, opts.memory_limit)),
            ..Default::default()
        });
        Runner { runtime, opts }
    }

    async fn run(mut self, module_spec: &ModuleSpecifier) -> Result<()> {
        // Terminate isolate when approaching memory limit
        let isolate_handle = self.runtime.v8_isolate().thread_safe_handle();
        self.runtime
            .add_near_heap_limit_callback(move |current_limit, _initial_limit| {
                println!(
                    "Terminating isolate near memory limit (current={current_limit} max={})",
                    self.opts.memory_limit
                );
                isolate_handle.terminate_execution();
                current_limit * 2
            });

        // Terminate isolate after timeout
        let isolate_handle = self.runtime.v8_isolate().thread_safe_handle();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _ = thread::spawn(move || {
            rt.block_on(async move {
                tokio::select! {
                    _ = tokio::time::sleep(self.opts.timeout) => {
                        println!("Terminating isolate after timeout ({:?})", self.opts.timeout);
                        isolate_handle.terminate_execution();
                    }
                }
            })
        });

        let module_id = self.runtime.load_main_module(module_spec, None).await?;
        let mut receiver = self.runtime.mod_evaluate(module_id);

        tokio::select! {
          biased;

          maybe_result = &mut receiver => {
            println!("Result = {maybe_result:#?}");
            maybe_result.expect("Module evaluation result not provided.")
          }

          event_loop_result = self.runtime.run_event_loop(false) => {
            event_loop_result?;
            let maybe_result = receiver.await;
            maybe_result.expect("Module evaluation result not provided.")
          }
        }
    }
}

#[post("/<name>")]
async fn exec(name: &str, args: &State<Args>) -> status::Custom<String> {
    let cwd = env::current_dir().unwrap();
    let module_spec = resolve_path(&format!("js/{name}.js"), &cwd).unwrap();

    let runner_opts = RunnerOpts {
        memory_limit: args.memory_limit * 1024 * 1024,
        timeout: Duration::from_millis(args.timeout),
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let res = thread::spawn(move || {
        let local = tokio::task::LocalSet::new();
        let runner = Runner::new(runner_opts);
        local.block_on(&rt, async move { runner.run(&module_spec).await })
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
