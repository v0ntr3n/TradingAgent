use clap::Parser;
use trading_agent_cli::{CliArgs, ProcessEnv, run_from_args};

#[tokio::main]
async fn main() {
    let args = CliArgs::parse();
    match run_from_args(args, &ProcessEnv).await {
        Ok(result) => {
            println!("TradingAgent final rating: {:?}", result.rating);
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
