use chrono::{Datelike, Local, NaiveDate, TimeZone};
use std::collections::HashMap;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use urlencoding::decode;

// http://127.0.0.1:8080/countdown?title=Italien&target=2025-07-22
// http://127.0.0.1:8080/countdown?title=Utomlandsstudier+i+Nederl%C3%A4nderna&target=2025-08-24
// https://days.debugg.co/countdown/f15b2636-dd9f-460f-8680-6d4d2bf992e2

#[derive(PartialEq)]
enum Status {
    Ok,
    NotFound,
    Redirect,
}

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();

    loop {
        let (socket, _) = listener.accept().await.unwrap();

        tokio::spawn(async move {
            handle_connection(socket).await;
        });
    }
}

async fn handle_connection(mut stream: TcpStream) {
    let mut buffer = [0; 1024];
    let bytes = stream.read(&mut buffer).await.unwrap();
    let request_str = String::from_utf8_lossy(&buffer[..bytes]);
    let request_line = request_str.lines().next().unwrap();
    let request = &decode(request_line).unwrap();

    println!("{}", request);

    let page = match get_page(request) {
        Some(page) => page,
        None => "/error",
    };

    let (status, content) = if let Some(params) = get_params(request) {
        match page {
            "/countdown" => countdown(params).await,
            _ => error("The page you are looking for does not exist.").await,
        }
    } else {
        match page {
            "/" => root().await,
            "/create" => create().await,
            "/test" => redirect("/").await, // Todo: remove this line
            "/form.css" => style("src/css/form.css").await,
            "/style.css" => style("src/css/style.css").await,
            _ => error("The page you are looking for does not exist.").await,
        }
    };

    let code = match status {
        Status::Ok => "HTTP/1.1 200 OK",
        Status::NotFound => "HTTP/1.1 404 Not Found",
        Status::Redirect => "HTTP/1.1 301 Moved Permanently",
    };

    let response = if status != Status::Redirect {
        format!(
            "{}\r\nContent-Length: {}\r\n\r\n{}",
            code,
            content.len(),
            content
        )
    } else {
        format!(
            "{}\r\nLocation: {}\r\nContent-Length: 0\r\n\r\n",
            code, content
        )
    };

    stream.write(response.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
}

fn get_page(request: &str) -> Option<&str> {
    let parts: Vec<&str> = request.split_whitespace().collect();

    if parts.len() < 3 {
        return None;
    }

    Some(parts[1].split('?').next()?)
}

// Todo: feels sketchy.
fn get_params(request: &str) -> Option<HashMap<&str, &str>> {
    let index0 = request.find('?')?;
    let index1 = request[index0..].rfind(' ')?;
    let query = &request[index0 + 1..index1 + index0];

    let mut params = HashMap::new();
    for param in query.split('&') {
        let pair: Vec<&str> = param.split('=').collect();

        if pair.len() < 2 {
            return None;
        }

        params.insert(pair[0], pair[1]);
    }

    Some(params)
}

async fn redirect(url: &str) -> (Status, String) {
    (Status::Redirect, url.to_string())
}

async fn root() -> (Status, String) {
    let mut result = fs::read_to_string("src/html/index.html").await.unwrap();

    let now = Local::now();
    let start_of_year = Local.with_ymd_and_hms(now.year(), 1, 1, 0, 0, 0).unwrap();
    let end_of_year = Local.with_ymd_and_hms(now.year(), 12, 31, 0, 0, 0).unwrap();

    let days_passed = (now - start_of_year).num_days() + 1;
    let days_in_year = (end_of_year - start_of_year).num_days() + 1;
    let percentage = ((days_passed as f32 / days_in_year as f32) * 100.0).floor();

    result = result.replace("__percentage__", &format!("{}", percentage));
    result = result.replace("__days_passed__", &format!("{}", days_passed));
    result = result.replace("__days_in_year__", &format!("{}", days_in_year));

    (Status::Ok, result)
}

async fn create() -> (Status, String) {
    let result = fs::read_to_string("src/html/create.html").await.unwrap();
    (Status::Ok, result)
}

async fn countdown(params: HashMap<&str, &str>) -> (Status, String) {
    let mut result = fs::read_to_string("src/html/countdown.html").await.unwrap();

    if !params.contains_key("title") || !params.contains_key("target") {
        return error("Invalid url parameters.").await;
    }

    let now = Local::now();
    let start_of_year = Local.with_ymd_and_hms(now.year(), 1, 1, 0, 0, 0).unwrap();
    let days_passed = (now - start_of_year).num_days();
    let target_date = NaiveDate::parse_from_str(&params["target"], "%Y-%m-%d").unwrap();
    let total_days = (target_date - start_of_year.date_naive()).num_days();
    let percentage = ((days_passed as f32 / total_days as f32) * 100.0).floor();

    result = result.replace("__title__", &params["title"].replace('+', " "));
    result = result.replace("__percentage__", &format!("{}", percentage));
    result = result.replace("__days_left__", &format!("{}", total_days - days_passed));
    result = result.replace("__target_date__", &format!("{}", &params["target"]));

    (Status::Ok, result)
}

async fn style(path: &str) -> (Status, String) {
    let result = match fs::read_to_string(path).await {
        Ok(x) => x,
        Err(e) => return error(&format!("Err: {}", e)).await,
    };

    (Status::Ok, result)
}

async fn error(message: &str) -> (Status, String) {
    let mut result = match fs::read_to_string("src/html/404.html").await {
        Ok(x) => x,
        Err(e) => return (Status::NotFound, format!("Err: {}", e)),
    };

    result = result.replace("__error__", message);
    return (Status::NotFound, result);
}
