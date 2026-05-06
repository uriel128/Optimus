mod analyzer;
mod ast;
mod lexer;
mod parser;

use chumsky::Parser;
use lexer::Token;
use logos::Logos;
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::process::{self, Command};
use std::time::{SystemTime, UNIX_EPOCH};

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let command = &args[1];

    if command == "repl" {
        run_repl();
        return;
    }

    if command == "server" || command == "serve" || command == "gui" {
        if command == "serve" {
            println!("Note: `serve` is deprecated. Use `server`.");
        }
        if let Err(err) = run_gui_server() {
            eprintln!("Error: GUI server failed: {}", err);
            process::exit(1);
        }
        return;
    }

    if !command.ends_with(".op") {
        eprintln!("Error: Optimus files must use the .op extension.");
        process::exit(1);
    }

    let source_code = match fs::read_to_string(command) {
        Ok(contents) => contents,
        Err(_) => {
            eprintln!("Error: Could not find or read file '{}'", command);
            process::exit(1);
        }
    };

    execute_source(command, &source_code);
}

fn print_usage() {
    println!("========================================");
    println!("Optimus Compiler & Complexity Analyzer");
    println!("========================================");
    println!("Usage:");
    println!("  cargo run -- <file.op>");
    println!("  cargo run -- repl");
    println!("  cargo run -- server");
}

fn run_repl() {
    println!("========================================");
    println!("Optimus REPL");
    println!("========================================");
    println!("Type Optimus code line-by-line and use:");
    println!("  :run   -> execute buffer");
    println!("  :show  -> print current buffer");
    println!("  :clear -> clear buffer");
    println!("  :quit  -> exit");

    let mut buffer = String::new();

    loop {
        print!("op> ");
        let _ = io::stdout().flush();

        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_err() {
            eprintln!("Error: failed to read input.");
            break;
        }

        let trimmed = line.trim();

        match trimmed {
            ":quit" => break,
            ":clear" => {
                buffer.clear();
                println!("Buffer cleared.");
            }
            ":show" => {
                if buffer.trim().is_empty() {
                    println!("(buffer is empty)");
                } else {
                    println!("{}", buffer);
                }
            }
            ":run" => {
                if buffer.trim().is_empty() {
                    println!("Buffer is empty.");
                } else {
                    execute_source("<repl>", &buffer);
                }
            }
            _ => {
                buffer.push_str(&line);
            }
        }
    }
}

fn run_gui_server() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7878")?;
    println!("========================================");
    println!("Optimus GUI Server");
    println!("========================================");
    println!("Open: http://127.0.0.1:7878");
    println!("Press Ctrl+C to stop.");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_connection(stream),
            Err(err) => eprintln!("Connection error: {}", err),
        }
    }

    Ok(())
}

fn handle_connection(mut stream: TcpStream) {
    let request = match read_http_request(&mut stream) {
        Ok(Some(req)) => req,
        Ok(None) => return,
        Err(err) => {
            let _ = write_http_response(
                &mut stream,
                "400 Bad Request",
                "text/plain; charset=utf-8",
                format!("Bad request: {}", err).as_bytes(),
            );
            return;
        }
    };

    let path = request.path.split('?').next().unwrap_or("/");

    match (request.method.as_str(), path) {
        ("GET", "/") | ("GET", "/index.html") => {
            let body = gui_html();
            let _ = write_http_response(
                &mut stream,
                "200 OK",
                "text/html; charset=utf-8",
                body.as_bytes(),
            );
        }
        ("POST", "/run") => {
            let code = match String::from_utf8(request.body) {
                Ok(content) => content,
                Err(_) => {
                    let _ = write_http_response(
                        &mut stream,
                        "400 Bad Request",
                        "text/plain; charset=utf-8",
                        b"Request body must be valid UTF-8.",
                    );
                    return;
                }
            };

            let output = run_code_for_gui(&code);
            let _ = write_http_response(
                &mut stream,
                "200 OK",
                "text/plain; charset=utf-8",
                output.as_bytes(),
            );
        }
        _ => {
            let _ = write_http_response(
                &mut stream,
                "404 Not Found",
                "text/plain; charset=utf-8",
                b"Not found",
            );
        }
    }
}

fn read_http_request(stream: &mut TcpStream) -> io::Result<Option<HttpRequest>> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 2048];
    let mut header_end = None;

    while header_end.is_none() {
        let bytes_read = stream.read(&mut chunk)?;
        if bytes_read == 0 {
            if buffer.is_empty() {
                return Ok(None);
            }
            break;
        }

        buffer.extend_from_slice(&chunk[..bytes_read]);
        header_end = find_header_end(&buffer);

        if buffer.len() > 2 * 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request too large",
            ));
        }
    }

    let header_end = match header_end {
        Some(pos) => pos,
        None => return Ok(None),
    };

    let headers_text = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = headers_text.split("\r\n");

    let request_line = lines.next().unwrap_or_default();
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default().to_string();
    let path = request_parts.next().unwrap_or("/").to_string();

    let mut content_length: usize = 0;
    for line in lines {
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }

    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let bytes_read = stream.read(&mut chunk)?;
        if bytes_read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..bytes_read]);
    }

    let available_body = buffer.len().saturating_sub(body_start);
    let body_len = available_body.min(content_length);
    let body = buffer[body_start..body_start + body_len].to_vec();

    Ok(Some(HttpRequest { method, path, body }))
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> io::Result<()> {
    let headers = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        body.len()
    );

    stream.write_all(headers.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn run_code_for_gui(source_code: &str) -> String {
    let mut temp_path = env::temp_dir();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    temp_path.push(format!(
        "optimus_gui_{}_{}.op",
        process::id(),
        timestamp
    ));

    if let Err(err) = fs::write(&temp_path, source_code) {
        return format!("Error: failed to write temp file: {}", err);
    }

    let exe = match env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            let _ = fs::remove_file(&temp_path);
            return format!("Error: failed to locate compiler executable: {}", err);
        }
    };

    let output = Command::new(exe).arg(&temp_path).output();
    let _ = fs::remove_file(&temp_path);

    match output {
        Ok(result) => {
            let mut combined = String::new();
            combined.push_str(&String::from_utf8_lossy(&result.stdout));

            if !result.stderr.is_empty() {
                if !combined.ends_with('\n') {
                    combined.push('\n');
                }
                combined.push_str("[stderr]\n");
                combined.push_str(&String::from_utf8_lossy(&result.stderr));
            }

            if combined.trim().is_empty() {
                combined.push_str("(no output)\n");
            }

            combined
        }
        Err(err) => format!("Error: failed to execute compiler: {}", err),
    }
}

fn gui_html() -> &'static str {
    r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Optimus GUI</title>
  <style>
    :root {
      --bg: #0f172a;
      --panel: #111827;
      --panel-2: #1f2937;
      --ink: #e5e7eb;
      --muted: #9ca3af;
      --accent: #22c55e;
      --accent-2: #f59e0b;
      --danger: #ef4444;
    }

    * { box-sizing: border-box; }

    body {
      margin: 0;
      font-family: "JetBrains Mono", "Fira Code", monospace;
      color: var(--ink);
      background: radial-gradient(circle at top left, #1e293b, var(--bg));
      min-height: 100vh;
      padding: 20px;
    }

    .wrap {
      max-width: 1200px;
      margin: 0 auto;
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 16px;
    }

    .card {
      background: rgba(17,24,39,0.85);
      border: 1px solid rgba(255,255,255,0.08);
      border-radius: 18px;
      overflow: hidden;
      backdrop-filter: blur(14px);
      box-shadow:
        0 0 0 1px rgba(255,255,255,0.03),
        0 10px 30px rgba(0,0,0,0.4),
        0 0 40px rgba(34,197,94,0.05);
      transition: all 0.25s ease;
    }

    .card:hover {
      transform: translateY(-2px);
      box-shadow:
        0 15px 40px rgba(0,0,0,0.5),
        0 0 50px rgba(34,197,94,0.08);
    }

    .head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 12px 14px;
      border-bottom: 1px solid rgba(255,255,255,0.1);
      background: rgba(17,24,39,0.7);
    }

    .title {
      font-size: 14px;
      letter-spacing: 0.5px;
      text-transform: uppercase;
      color: var(--muted);
    }

    .actions {
      display: flex;
      gap: 8px;
    }

    button {
      border: 0;
      border-radius: 10px;
      padding: 10px 16px;
      font-weight: 700;
      cursor: pointer;
      font-family: inherit;
      transition: all 0.2s ease;
    }
    
    button:hover {
      transform: translateY(-1px);
      opacity: 0.95;
    }
    
    button:active {
      transform: scale(0.98);
    }

    #runBtn { background: var(--accent); color: #052e16; }
    #demoBtn { background: var(--accent-2); color: #451a03; }
    #clearBtn { background: var(--danger); color: #450a0a; }

    textarea {
      width: 100%;
      min-height: 72vh;
      border: 0;
      margin: 0;
      padding: 14px;
      resize: vertical;
      background: var(--panel);
      color: var(--ink);
      font-family: inherit;
      font-size: 14px;
      line-height: 1.45;
    }

    pre {
      margin: 0;
      padding: 18px;
      min-height: 72vh;
      background:
        radial-gradient(circle at top, #1e293b, #111827);
      color: #d1fae5;
      overflow: auto;
      font-size: 14px;
      line-height: 1.6;
      white-space: pre-wrap;
      word-break: break-word;
    }

    .status {
      padding: 10px 14px;
      color: var(--muted);
      border-top: 1px solid rgba(255,255,255,0.08);
      font-size: 12px;
    }

    .complexity-report {
      display: flex;
      flex-direction: column;
      gap: 10px;
      margin-top: 18px;
    }

    .metric {
      font-weight: bold;
      padding: 10px 14px;
      border-radius: 10px;
      font-size: 15px;
      letter-spacing: 0.3px;
      display: block;
    }

    .time {
      background: rgba(59,130,246,0.15);
      border: 1px solid #3b82f6;
      color: #93c5fd;
    }

    .space {
      background: rgba(168,85,247,0.15);
      border: 1px solid #a855f7;
      color: #d8b4fe;
    }

    .ops {
      background: rgba(34,197,94,0.15);
      border: 1px solid #22c55e;
      color: #86efac;
    }

    .alloc {
      background: rgba(245,158,11,0.15);
      border: 1px solid #f59e0b;
      color: #fcd34d;
    }

    .hero {
      text-align: center;
      margin-bottom: 24px;
    }
    
    .hero h1 {
      margin: 0;
      font-size: 52px;
      letter-spacing: 6px;
      color: #22c55e;
      text-shadow: 0 0 20px rgba(34,197,94,0.4);
    }
    
    .hero p {
      margin-top: 6px;
      color: #94a3b8;
    }

    @media (max-width: 1000px) {
      .wrap { grid-template-columns: 1fr; }
      textarea, pre { min-height: 46vh; }
    }
  </style>
</head>
<body>
    <div class="hero">
    <h1>OPTIMUS</h1>
    <p>Programming Language + Complexity Analyzer</p>
  </div>

  <div class="wrap">
    <section class="card">
      <div class="head">
        <div class="title">Optimus Source</div>
        <div class="actions">
          <button id="demoBtn" type="button">Load Demo</button>
          <button id="clearBtn" type="button">Clear</button>
          <button id="runBtn" type="button">Run</button>
        </div>
      </div>
      <textarea id="code" spellcheck="false"></textarea>
      <div class="status" id="status">Ready</div>
    </section>

    <section class="card">
      <div class="head">
        <div class="title">Execution Output</div>
      </div>
      <pre id="output">Click Run to execute your Optimus program.</pre>
      <div class="status">Local-only execution on your machine</div>
    </section>
  </div>

  <script>
    const demo = `module MathKit {
  fn square(int n): int {
    return n * n;
  }
}

import MathKit;

class Counter {
  mut int value = 0;

  fn init(int start): void {
    self.value = start;
    return;
  }

  fn inc(int by): int {
    self.value = self.value + by;
    return self.value;
  }
}

mut Counter c = new Counter(10);
print(MathKit.square(6));
print(c.inc(5));`;

    const code = document.getElementById('code');
    const output = document.getElementById('output');
    const status = document.getElementById('status');

    document.getElementById('demoBtn').addEventListener('click', () => {
      code.value = demo;
      status.textContent = 'Demo loaded';
    });

    document.getElementById('clearBtn').addEventListener('click', () => {
      code.value = '';
      output.textContent = 'Cleared.';
      status.textContent = 'Ready';
    });

    document.getElementById('runBtn').addEventListener('click', async () => {
      status.textContent = 'Running...';
      output.textContent = 'Running...';

      try {
        const res = await fetch('/run', {
          method: 'POST',
          headers: { 'Content-Type': 'text/plain; charset=utf-8' },
          body: code.value
        });

        const text = await res.text();
        output.innerHTML = text;
        status.textContent = res.ok ? 'Done' : 'Error';
      } catch (err) {
        output.textContent = String(err);
        status.textContent = 'Error';
      }
    });
  </script>
</body>
</html>
"#
}

fn execute_source(name: &str, source_code: &str) {
    println!("Analyzing: {}", name);
    println!("----------------------------------------");

    let (tokens, lex_errors) = lex_source(source_code);
    if !lex_errors.is_empty() {
        println!("Lexing Errors Found:");
        for span in lex_errors {
            println!("  - invalid token at byte range {}..{}", span.start, span.end);
        }
        return;
    }

    let (ast, errors) = parser::parser().parse_recovery(tokens);

    if !errors.is_empty() {
        println!("Syntax Errors Found:");
        for err in errors {
            let found = err
                .found()
                .map(|token| format!("{:?}", token))
                .unwrap_or_else(|| "<eof>".to_string());
            let expected: Vec<String> = err
                .expected()
                .filter_map(|tok| tok.as_ref().map(|t| format!("{:?}", t)))
                .collect();

            if expected.is_empty() {
                println!("  - found unexpected token {}", found);
            } else {
                println!(
                    "  - found {}, expected one of: {}",
                    found,
                    expected.join(", ")
                );
            }
        }
        return;
    }

    if let Some(tree) = ast {
        println!("Syntax Validated!");
        let mut analyzer = analyzer::Analyzer::new();
        analyzer.analyze(&tree);
    }
}

fn lex_source(source: &str) -> (Vec<Token>, Vec<Range<usize>>) {
    let mut lexer = Token::lexer(source);
    let mut tokens = Vec::new();
    let mut errors = Vec::new();

    while let Some(token) = lexer.next() {
        match token {
            Ok(tok) => tokens.push(tok),
            Err(_) => errors.push(lexer.span()),
        }
    }

    (tokens, errors)
}
