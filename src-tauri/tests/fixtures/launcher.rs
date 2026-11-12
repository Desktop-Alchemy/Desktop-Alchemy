#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--scene-probe") {
        let mut byte = [0u8; 1];
        let _ = std::io::Read::read_exact(&mut std::io::stdin(), &mut byte);
        return;
    }
    if let Some(output) = args.first() {
        let value = serde_json::json!({ "arguments": &args[1..], "directory": std::env::current_dir().unwrap() });
        std::fs::write(output, serde_json::to_vec(&value).unwrap()).unwrap();
    }
}
