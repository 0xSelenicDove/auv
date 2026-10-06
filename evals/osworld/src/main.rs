use std::path::PathBuf;

#[tokio::main]
async fn main() {
  let mut args = std::env::args_os();
  let _program = args.next();
  let result = match (args.next(), args.next(), args.next()) {
    (Some(flag), Some(path), None) if flag == "--plan" => auv_osworld_evals::entry::run(&PathBuf::from(path)).await,
    _ => Err("usage: auv-osworld-action --plan ABSOLUTE_OPERATOR_AUDITED_JSON".into()),
  };
  if let Err(error) = result {
    eprintln!("{error}");
    std::process::exit(1);
  }
}
