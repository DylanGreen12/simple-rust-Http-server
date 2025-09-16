use std::{
    io::{prelude::*, BufReader},
    net::{TcpListener, TcpStream},
    fs,
    path::{Path, PathBuf},
    env,
};

fn main() {
    // Set the server address and port
    let server_address = "127.0.0.1:8080";
    
    // Determine the root directory for serving files
    let pages_dir = get_pages_directory();
    println!("Server running on http://{}", server_address);
    println!("Serving files from: {:?}", pages_dir);
    
    // Verify the pages directory exists
    if !pages_dir.exists() {
        eprintln!("ERROR: Pages directory does not exist: {:?}", pages_dir);
        eprintln!("Please create a 'pages' folder with web files");
        return;
    }
    
    // Try to bind to the address, with error handling
    let listener = TcpListener::bind(server_address).expect("Failed to bind to address");
    
    // Handle connections sequentially
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let pages_dir = pages_dir.clone();
                handle_connection(stream, &pages_dir);
            }
            Err(e) => {
                eprintln!("Connection failed: {}", e);
            }
        }
    }
}

// Fix exe file pathing
fn get_pages_directory() -> PathBuf {
    // First, try to find the pages directory next to the executable
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Check if we're running from a development environment (target/debug)
            let project_root = if exe_dir.ends_with("target/debug") || exe_dir.ends_with("target/release") {
                exe_dir.parent().unwrap().parent().unwrap().to_path_buf()
            } else {
                exe_dir.to_path_buf()
            };
            
            let pages_dir = project_root.join("pages");
            return pages_dir;
        }
    }
    
    // Final fallback: current directory pages folder
    env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join("pages")
}

// Process connections, handle requests, serve files
fn handle_connection(mut stream: TcpStream, pages_dir: &Path) {
    let buf_reader = BufReader::new(&mut stream);
    let http_request: Vec<_> = buf_reader
        .lines()
        .map(|result| result.unwrap())
        .take_while(|line| !line.is_empty())
        .collect();
    
    // Print the request to terminal
    println!("=== HTTP Request Received ===");
    for line in &http_request {
        println!("{}", line);
    }
    println!("=============================");
    
    // Parse the request line (first line)
    let request_line = http_request.first().unwrap();
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    
    if parts.len() < 2 {
        send_error_response(&mut stream, "400 Bad Request", "Bad Request", pages_dir, false);
        return;
    }
    
    let method = parts[0];
    let mut path = parts[1];
    
    // Only handle GET requests
    if method != "GET" {
        send_error_response(&mut stream, "405 Method Not Allowed", "Method Not Allowed", pages_dir, false);
        return;
    }
    
    // Handle root path
    if path == "/" {
        path = "/index.html";
    }
    
    // Security: Prevent directory traversal attacks, 403
    if path.contains("..") {
        println!("Blocked directory traversal attempt: {}", path);
        send_error_response(&mut stream, "403 Forbidden", "Directory traversal not allowed", pages_dir, true);
        return;
    }
    
    // Remove leading slash and build full path
    let filename = &path[1..]; 
    let full_path = pages_dir.join(filename);
    
    // Check if file exists
    if !full_path.exists() {
        println!("File not found: {}", filename);
        send_error_response(&mut stream, "404 Not Found", "File Not Found", pages_dir, true);
        return;
    }
    
    // Read the file content
    let contents = match fs::read_to_string(&full_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading file {:?}: {}", full_path, e);
            send_error_response(&mut stream, "500 Internal Server Error", "Error reading file", pages_dir, false);
            return;
        }
    };
    
    // Check for Connection: keep-alive header
    let mut connection_header = "close"; 
    for line in &http_request {
        if line.to_lowercase().starts_with("connection:") {
            if line.to_lowercase().contains("keep-alive") {
                connection_header = "keep-alive";
            }
            break;
        }
    }
    
    // Determine content type based on file extension
    let content_type = get_content_type(filename);
    
    // Build response
    let length = contents.len();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n{}",
        content_type, length, connection_header, contents
    );
    
    // Print response headers to terminal (without body)
    println!("=== HTTP Response Sent ===");
    let response_lines: Vec<&str> = response.split("\r\n").collect();
    for line in &response_lines[..response_lines.len().saturating_sub(1)] {
        if !line.is_empty() {
            println!("{}", line);
        }
    }
    println!("===========================");
    
    // Send response
    if let Err(e) = stream.write_all(response.as_bytes()) {
        eprintln!("Failed to send response: {}", e);
    }
}

fn send_error_response(stream: &mut TcpStream, status: &str, message: &str, pages_dir: &Path, try_html: bool) {
    let (content, content_type) = if try_html {
        // Check if there's a custom error page for this status code
        let (status_code, _) = status.split_once(' ').unwrap_or((status, ""));
        let error_page_path = pages_dir.join(format!("{}.html", status_code));
        
        if error_page_path.exists() {
            // Serve the custom error page
            match fs::read_to_string(&error_page_path) {
                Ok(content) => (content, "text/html"),
                Err(_) => (message.to_string(), "text/plain"),
            }
        } else {
            // Fall back to plain text message
            (message.to_string(), "text/plain")
        }
    } else {
        // Use plain text for non-HTML errors
        (message.to_string(), "text/plain")
    };
    
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        content_type,
        content.len(),
        content
    );
    
    // Print error response to terminal
    println!("=== HTTP Error Response ===");
    let response_lines: Vec<&str> = response.split("\r\n").collect();
    for line in &response_lines[..response_lines.len().saturating_sub(1)] {
        if !line.is_empty() {
            println!("{}", line);
        }
    }
    println!("===========================");
    
    if let Err(e) = stream.write_all(response.as_bytes()) {
        eprintln!("Failed to send error response: {}", e);
    }
}

// Handle more MIME types
fn get_content_type(filename: &str) -> &str {
    if filename.ends_with(".html") {
        "text/html"
    } else if filename.ends_with(".css") {
        "text/css"
    } else if filename.ends_with(".js") {
        "application/javascript"
    } else if filename.ends_with(".png") {
        "image/png"
    } else if filename.ends_with(".jpg") || filename.ends_with(".jpeg") {
        "image/jpeg"
    } else if filename.ends_with(".gif") {
        "image/gif"
    } else if filename.ends_with(".svg") {
        "image/svg+xml"
    } else if filename.ends_with(".ico") {
        "image/x-icon"
    } else if filename.ends_with(".txt") {
        "text/plain"
    } else if filename.ends_with(".pdf") {
        "application/pdf"
    } else {
        "application/octet-stream"
    }
}
