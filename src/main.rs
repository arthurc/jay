use jay::{JayError, Runtime};

fn main() -> Result<(), JayError> {
    let runtime = Runtime::new();
    runtime.run_with_main("com.example.Test")?;
    Ok(())
}
