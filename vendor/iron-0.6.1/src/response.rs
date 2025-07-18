//! Iron's HTTP Response representation and associated methods.

use std::io::{self, Write};
use std::fmt::{self, Debug};
use std::fs::File;

use modifier::{Set, Modifier};
use hyper::header::Headers;

use status::{self, Status};
use headers;

pub use hyper::server::response::Response as HttpResponse;
use hyper::net::Fresh;

/// Wrapper type to set `Read`ers as response bodies
pub struct BodyReader<R: Send>(pub R);

/// A trait which writes the body of an HTTP response.
pub trait WriteBody: Send {
    /// Writes the body to the provided `Write`.
    fn write_body(&mut self, res: &mut Write) -> io::Result<()>;
}

impl WriteBody for String {
    fn write_body(&mut self, res: &mut Write) -> io::Result<()> {
        self.as_bytes().write_body(res)
    }
}

impl<'a> WriteBody for &'a str {
    fn write_body(&mut self, res: &mut Write) -> io::Result<()> {
        self.as_bytes().write_body(res)
    }
}

impl WriteBody for Vec<u8> {
    fn write_body(&mut self, res: &mut Write) -> io::Result<()> {
        res.write_all(self)
    }
}

impl<'a> WriteBody for &'a [u8] {
    fn write_body(&mut self, res: &mut Write) -> io::Result<()> {
        res.write_all(self)
    }
}

impl WriteBody for File {
    fn write_body(&mut self, res: &mut Write) -> io::Result<()> {
        io::copy(&mut std::io::BufReader::with_capacity(1024 * 1024, self), res).map(|_| ())
    }
}

impl WriteBody for Box<io::Read + Send> {
    fn write_body(&mut self, res: &mut Write) -> io::Result<()> {
        io::copy(&mut std::io::BufReader::with_capacity(1024 * 1024, self), res).map(|_| ())
    }
}

impl <R: io::Read + Send> WriteBody for BodyReader<R> {
    fn write_body(&mut self, res: &mut Write) -> io::Result<()> {
        io::copy(&mut std::io::BufReader::with_capacity(1024 * 1024, &mut self.0), res).map(|_| ())
    }
}

/* Needs specialization :(
impl<R: Read + Send> WriteBody for R {
    fn write_body(&mut self, res: &mut Write) -> io::Result<()> {
        io::copy(self, res)
    }
}
*/

/// The response representation given to `Middleware`
pub struct Response {
    /// The response status-code.
    pub status: Option<Status>,

    /// The headers of the response.
    pub headers: Headers,

    /// The body of the response.
    pub body: Option<Box<WriteBody>>
}

impl Response {
    /// Construct a blank Response
    pub fn new() -> Response {
        Response {
            status: None, // Start with no response code.
            body: None, // Start with no body.
            headers: Headers::new(),
        }
    }

    /// Construct a Response with the specified modifier pre-applied.
    pub fn with<M: Modifier<Response>>(m: M) -> Response {
        Response::new().set(m)
    }

    // `write_back` is used to put all the data added to `self`
    // back onto an `HttpResponse` so that it is sent back to the
    // client.
    //
    // `write_back` consumes the `Response`.
    #[doc(hidden)]
    pub fn write_back(self, mut http_res: HttpResponse<Fresh>) {
        *http_res.headers_mut() = self.headers;

        // Default to a 404 if no response code was set
        *http_res.status_mut() = self.status.unwrap_or(status::NotFound);

        let out = match self.body {
            Some(body) => write_with_body(http_res, body),
            None => {
                if !http_res.headers().has::<headers::ContentLength>() {
                    http_res.headers_mut().set(headers::ContentLength(0));
                }
                http_res.start().and_then(|res| res.end())
            }
        };

        if let Err(e) = out {
            eprintln!("[iron] Error writing response: {}", e);
        }
    }
}

fn write_with_body(res: HttpResponse<Fresh>, mut body: Box<WriteBody>)
                   -> io::Result<()> {
    let mut raw_res = try!(res.start());
    if let Err(e) = body.write_body(&mut raw_res.writer()) {
        if e.kind() != std::io::ErrorKind::WriteZero {
            try!(Err(e));
        }
    }
    raw_res.end()
}

impl Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "HTTP/1.1 {}\n{}",
            self.status.unwrap_or(status::NotFound),
            self.headers
        )
    }
}

impl fmt::Display for Response {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(self, f)
    }
}

impl Set for Response {}
