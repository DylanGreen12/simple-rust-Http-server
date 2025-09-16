use std::{
    io::{prelude::*, BufReader},
    net::{TcpListener, TcpStream},
    fs,
    path::{Path, PathBuf},
    thread,
    env,
};

fn main() {
    // Set the server address and port
    let server_address = "127.0.0.1:8080";
    
    // Determine the root directory for serving files
    let root_dir = get_root_directory();
    println!("Serving files from: {:?}", root_dir);
    
    // Verify the pages directory exists
    if !root_dir.exists() {
        eprintln!("ERROR: Pages directory does not exist: {:?}", root_dir);
        eprintln!("Please make sure the 'pages' folder is in the same directory as the executable");
        return;
    }
    
    // Try to bind to the address, with error handling
    let listener = match TcpListener::bind(server_address) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("Failed to bind to {}: {}", server_address, e);
            return;
        }
    };
    
    println!("Server running on http://{}", server_address);
    println!("Press Ctrl+C to stop the server");
    
    // Handle connections sequentially
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let root_dir = root_dir.clone();
                // Handle each connection in a separate thread
                thread::spawn(move || {
                    handle_connection(stream, &root_dir);
                });
            }
            Err(e) => {
                eprintln!("Connection failed: {}", e);
            }
        }
    }
}

fn get_root_directory() -> PathBuf {
    // Try to get the root directory from command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        return PathBuf::from(&args[1]);
    }
    
    // First, try to find the pages directory next to the executable
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let pages_dir = exe_dir.join("pages");
            if pages_dir.exists() {
                return pages_dir;
            }
            
            // If no pages directory, check if we're running from a development environment
            let project_pages = exe_dir.parent().unwrap().parent().unwrap().join("pages");
            if project_pages.exists() {
                return project_pages;
            }
            
            // Fallback: use the executable directory itself
            return exe_dir.to_path_buf();
        }
    }
    
    // Final fallback: current directory
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn handle_connection(mut stream: TcpStream, root_dir: &Path) {
    let buf_reader = BufReader::new(&mut stream);
    let request_line = match buf_reader.lines().next() {
        Some(Ok(line)) => line,
        Some(Err(e)) => {
            eprintln!("Error reading request: {}", e);
            send_error_response(&mut stream, "400 Bad Request", "Bad Request", root_dir);
            return;
        }
        None => {
            eprintln!("Empty request received");
            send_error_response(&mut stream, "400 Bad Request", "Bad Request", root_dir);
            return;
        }
    };
    
    // Parse the request
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        send_error_response(&mut stream, "400 Bad Request", "Bad Request", root_dir);
        return;
    }
    
    let method = parts[0];
    let mut path = parts[1];
    
    // Only handle GET requests
    if method != "GET" {
        send_error_response(&mut stream, "405 Method Not Allowed", "Method Not Allowed", root_dir);
        return;
    }
    
    // Handle root path
    if path == "/" {
        path = "/index.html";
    }
    
    // Security: Prevent directory traversal attacks
    if path.contains("..") {
        send_error_response(&mut stream, "403 Forbidden", "Directory traversal not allowed", root_dir);
        return;
    }
    
    // Remove leading slash and build full path
    let filename = &path[1..]; // Remove the leading '/'
    let full_path = root_dir.join(filename);
    
    println!("Looking for file: {:?}", full_path);
    
    if !full_path.exists() {
        eprintln!("File not found: {:?}", full_path);
        send_error_response(&mut stream, "404 Not Found", "File Not Found", root_dir);
        return;
    }
    
    // Read the file content
    let contents = match fs::read_to_string(&full_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading file {:?}: {}", full_path, e);
            send_error_response(&mut stream, "500 Internal Server Error", "Error reading file", root_dir);
            return;
        }
    };
    
    // Determine content type based on file extension
    let content_type = get_content_type(filename);
    
    // Build response
    let length = contents.len();
    let response = format!(
        "HTTP/1.0 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n{}",
        content_type, length, contents
    );
    
    // Send response
    if let Err(e) = stream.write_all(response.as_bytes()) {
        eprintln!("Failed to send response: {}", e);
    }
}

fn send_error_response(stream: &mut TcpStream, status: &str, message: &str, root_dir: &Path) {
    // Check if there's a custom error page for this status code
    let (status_code, _) = status.split_once(' ').unwrap_or((status, ""));
    let error_page_path = root_dir.join(format!("{}.html", status_code));
    
    let (content, content_type) = if error_page_path.exists() {
        // Serve the custom error page
        match fs::read_to_string(&error_page_path) {
            Ok(content) => (content, "text/html"),
            Err(_) => (message.to_string(), "text/plain"),
        }
    } else {
        // Fall back to plain text message
        (message.to_string(), "text/plain")
    };
    
    let response = format!(
        "HTTP/1.0 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n{}",
        status,
        content_type,
        content.len(),
        content
    );
    
    if let Err(e) = stream.write_all(response.as_bytes()) {
        eprintln!("Failed to send error response: {}", e);
    }
}

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
    } else {
        "text/plain"
    }
}