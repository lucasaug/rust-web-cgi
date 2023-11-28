use http::{
    Response,
    StatusCode, Request,
};

use std::{
    collections::HashMap,
    str::{
        FromStr,
        Lines,
    },
    net::TcpStream,
};

use log::debug;

use crate::http_server::{request::{static_request::static_handler::StaticRequestHandler, request::RequestHandler}, response::generate_error_response};

#[derive(strum_macros::EnumString, Eq, Hash, PartialEq, Debug)]
#[strum(serialize_all = "Train-Case", ascii_case_insensitive)]
pub enum CGIResponseHeader {
    ContentType,
    Location,
    Status,
}

pub type CGIResponseHeaderMap = HashMap<CGIResponseHeader, String>;

pub struct CGIScriptResponse {
    headers: CGIResponseHeaderMap,
    body: String,
}

impl CGIScriptResponse {
    fn new(headers: CGIResponseHeaderMap, body: String) -> CGIScriptResponse {
        CGIScriptResponse { headers, body }
    }
}

fn parse_cgi_headers(cgi_output: &mut Lines) ->
    Result<CGIResponseHeaderMap, ()> {

     let mut headers = CGIResponseHeaderMap::new();

     loop {
        let next_line = if let Some(line_result) = cgi_output.next() {
            line_result
        } else {
            debug!("Malformed CGI response");
            return Err(());
        };

        if next_line.is_empty() {
            break;
        }

        let split_line = next_line.split_once(":");
        match split_line {
            None => {
                debug!("Invalid CGI header");
                return Err(());
            },
            Some((before, after)) => {
                let header_value = CGIResponseHeader::from_str(before);

                if let Ok(header_key) = header_value {
                    headers.insert(header_key, after.to_string());
                } else if let Err(_) = header_value {
                    debug!("Couldn't parse header: {:?}", before);
                }
            }
        }
    }   

    Ok(headers)
}

pub fn parse_cgi_response(
    cgi_output: String
) -> Result<CGIScriptResponse, ()> {
    let mut output_lines = cgi_output.lines();
    let response_headers = parse_cgi_headers(&mut output_lines);
    let response_headers = match response_headers {
        Err(_) => {
            return Err(())
        },
        Ok(headers) => headers
    };

    let response_body = output_lines.collect::<String>();
    Ok(CGIScriptResponse::new(response_headers, response_body))
}


fn local_redirect(
    stream: &TcpStream, 
    static_handler: &StaticRequestHandler,
    location: &str,
) -> Response<String> {
    let static_request = Request::builder()
        .method("GET")
        .uri(location)
        .body(String::from(""));

    match static_request {
        Err(_) => generate_error_response(
            StatusCode::INTERNAL_SERVER_ERROR
        ),
        Ok(static_request) => {
            let response = static_handler.handle_request(
                stream,
                &static_request
            );

            match response {
                None => generate_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR
                ),
                Some(response) => response
            }
        }
    }
}

fn client_redirect(location: &str) -> Response<String> {
    let response = Response::builder().status(StatusCode::FOUND)
        .header("location", location)
        .body(String::from(""));

    match response {
        Err(_) => generate_error_response(StatusCode::INTERNAL_SERVER_ERROR),
        Ok(response) => response
    }
}

fn document_response(
    headers: CGIResponseHeaderMap,
    body: String
) -> Response<String> {
    let status = match headers.get(&CGIResponseHeader::Status) {
        None => String::from(StatusCode::OK.as_str()),
        Some(status) => status.clone()
    };

    let status = match StatusCode::from_str(status.as_str()) {
        Err(_) => return generate_error_response(
            StatusCode::INTERNAL_SERVER_ERROR
        ),
        Ok(value) => value
    };

    let content_type = match headers.get(&CGIResponseHeader::ContentType) {
        None => return generate_error_response(
            StatusCode::INTERNAL_SERVER_ERROR
        ),
        Some(value) => value
    };

    let response = Response::builder()
        .status(status)
        .header("content-type", content_type)
        .body(body);

    match response {
        Err(_) => generate_error_response(StatusCode::INTERNAL_SERVER_ERROR),
        Ok(response) => response
    }
}

pub fn convert_cgi_response_to_http(
    stream: &TcpStream, 
    static_handler: &StaticRequestHandler,
    cgi_response: CGIScriptResponse,
) -> Response<String> {
    let response_headers = cgi_response.headers;
    let response_body = cgi_response.body;

    if response_headers.contains_key(&CGIResponseHeader::Location) {
        let location = &response_headers[&CGIResponseHeader::Location];
        if location.starts_with("/") {
            local_redirect(stream, static_handler, location)
        } else {
            client_redirect(location)
        }
    } else {
        document_response(response_headers, response_body)
    }
}

