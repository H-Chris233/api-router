mod parser;
mod plan;
mod response;
mod routes;
mod router;

pub use router::handle_request;

#[cfg(test)]
mod tests;
