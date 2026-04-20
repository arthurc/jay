use std::env;
use std::process::ExitCode;

use jay::cli;
use jay::vm::Vm;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("jay: {error}");
            for frame in error.java_stack_trace() {
                eprintln!(
                    "  at {}.{}{} (pc {})",
                    frame.class_name, frame.method_name, frame.descriptor, frame.pc
                );
            }
            ExitCode::FAILURE
        }
    }
}

fn run() -> jay::JayResult<()> {
    let config = cli::parse_args(env::args().skip(1))?;
    Vm::new(config.classpath)?.run_main(&config.main_class)
}
