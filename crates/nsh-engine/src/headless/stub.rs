//! Stand-ins for a device's services in tests: a network that answers
//! from a list ([`StubNetwork`]) and storage that starts with given entries
//! ([`MemoryStorage`]).

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use nsh_core::storage::{StorageBackend, StorageChange};

use crate::{FetchEvent, FetchEvents, FetchTask, HttpBody, HttpHead, HttpRequest, Network};

/// A response [`StubNetwork`] gives.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StubResponse {
    /// The status code.
    pub status: u16,
    /// Header names and values.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Vec<u8>,
}

/// `fetch` that answers from a list of responses, by method and URL; a
/// request it has none for fails, so a test never reaches the network
/// unasked.
#[derive(Debug, Default)]
pub struct StubNetwork {
    responses: Vec<(String, String, StubResponse)>,
    requests: Mutex<Vec<HttpRequest>>,
}

impl StubNetwork {
    /// A network with `responses`, each keyed `"<METHOD> <URL>"` (`"GET
    /// https://api.example.com/todos"`; a bare URL is a `GET`); the URL is
    /// matched whole, its query included.
    pub fn new(responses: impl IntoIterator<Item = (String, StubResponse)>) -> Self {
        let responses = responses
            .into_iter()
            .map(|(key, response)| {
                let (method, url) = match key.split_once(' ') {
                    Some((method, url))
                        if !method.is_empty()
                            && method.chars().all(|c| c.is_ascii_alphabetic()) =>
                    {
                        (method.to_ascii_uppercase(), url.trim().to_owned())
                    }
                    _ => ("GET".to_owned(), key.trim().to_owned()),
                };
                (method, url, response)
            })
            .collect();
        Self {
            responses,
            requests: Mutex::new(Vec::new()),
        }
    }

    /// The requests made so far, in order (bodies streamed by JS are not
    /// kept).
    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().expect("requests").clone()
    }

    fn response(&self, request: &HttpRequest) -> Option<&StubResponse> {
        self.responses
            .iter()
            .find(|(method, url, _)| *method == request.method && *url == request.url)
            .map(|(_, _, response)| response)
    }
}

impl Network for StubNetwork {
    fn fetch(&self, request: HttpRequest, mut events: FetchEvents) -> Box<dyn FetchTask> {
        self.requests
            .lock()
            .expect("requests")
            .push(request.clone());
        let answer = match self.response(&request) {
            Some(response) => Answer::Response(request.url.clone(), response.clone()),
            None => Answer::Failed(format!(
                "{} {}: no response is listed for it",
                request.method, request.url
            )),
        };
        if request.body == HttpBody::Stream {
            // The body is taken first, as a server would.
            events(FetchEvent::BodyWanted);
            return Box::new(StubTask {
                events: Some(events),
                answer: Some(answer),
            });
        }
        answer.send(&mut events);
        Box::new(StubTask {
            events: None,
            answer: None,
        })
    }
}

enum Answer {
    Response(String, StubResponse),
    Failed(String),
}

impl Answer {
    fn send(self, events: &mut FetchEvents) {
        match self {
            Answer::Response(url, response) => {
                events(FetchEvent::Head(HttpHead {
                    status: response.status,
                    status_text: String::new(),
                    url,
                    redirected: false,
                    headers: response.headers,
                }));
                if !response.body.is_empty() {
                    events(FetchEvent::Data(response.body));
                }
                events(FetchEvent::End);
            }
            Answer::Failed(message) => events(FetchEvent::Failed(message)),
        }
    }
}

/// A request of [`StubNetwork`] whose body JS still streams: answered once
/// the body is complete.
struct StubTask {
    events: Option<FetchEvents>,
    answer: Option<Answer>,
}

impl FetchTask for StubTask {
    fn send(&mut self, _chunk: Vec<u8>) {
        if let Some(events) = &mut self.events {
            events(FetchEvent::BodyWanted);
        }
    }

    fn finish(&mut self) {
        if let (Some(mut events), Some(answer)) = (self.events.take(), self.answer.take()) {
            answer.send(&mut events);
        }
    }

    fn pause(&mut self, _paused: bool) {}

    fn cancel(self: Box<Self>) {}
}

/// Storage in memory that starts with the entries given: an app's data as
/// a test sets it up. Shared ([`Arc`]), it shows what the app wrote.
#[derive(Debug, Default)]
pub struct MemoryStorage {
    entries: Mutex<BTreeMap<String, String>>,
}

impl MemoryStorage {
    /// Storage holding `entries`.
    pub fn new(entries: impl IntoIterator<Item = (String, String)>) -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(entries.into_iter().collect()),
        })
    }

    /// The entries now.
    pub fn entries(&self) -> BTreeMap<String, String> {
        self.entries.lock().expect("entries").clone()
    }
}

impl StorageBackend for MemoryStorage {
    fn load(&self) -> Result<BTreeMap<String, String>, String> {
        Ok(self.entries())
    }

    fn write(&self, change: StorageChange) {
        let mut entries = self.entries.lock().expect("entries");
        match change {
            StorageChange::Set(key, value) => {
                entries.insert(key, value);
            }
            StorageChange::Remove(key) => {
                entries.remove(&key);
            }
            StorageChange::Clear => entries.clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fetch(network: &StubNetwork, request: HttpRequest) -> Vec<FetchEvent> {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let mut task = network.fetch(request, Box::new(move |e| sink.lock().unwrap().push(e)));
        task.finish();
        events.lock().unwrap().clone()
    }

    #[test]
    fn listed_requests_are_answered_and_others_fail() {
        let network = StubNetwork::new([
            (
                "https://api.test/todos".to_owned(),
                StubResponse {
                    status: 200,
                    headers: vec![("content-type".into(), "application/json".into())],
                    body: b"[]".to_vec(),
                },
            ),
            (
                "post https://api.test/todos".to_owned(),
                StubResponse {
                    status: 201,
                    ..Default::default()
                },
            ),
        ]);
        let events = fetch(&network, HttpRequest::get("https://api.test/todos"));
        assert!(matches!(&events[0], FetchEvent::Head(h) if h.status == 200));
        assert_eq!(events[1], FetchEvent::Data(b"[]".to_vec()));
        assert_eq!(events[2], FetchEvent::End);
        let events = fetch(&network, HttpRequest::post("https://api.test/todos", "x"));
        assert!(matches!(&events[0], FetchEvent::Head(h) if h.status == 201));
        assert_eq!(events[1], FetchEvent::End);
        let events = fetch(&network, HttpRequest::get("https://api.test/todos?all"));
        assert_eq!(
            events,
            [FetchEvent::Failed(
                "GET https://api.test/todos?all: no response is listed for it".into()
            )]
        );
        let mut streamed = HttpRequest::new("POST", "https://api.test/todos");
        streamed.body = HttpBody::Stream;
        let events = fetch(&network, streamed);
        assert_eq!(events[0], FetchEvent::BodyWanted);
        assert!(matches!(&events[1], FetchEvent::Head(h) if h.status == 201));
        assert_eq!(network.requests().len(), 4);
    }

    #[test]
    fn storage_starts_with_its_entries_and_keeps_changes() {
        let storage = MemoryStorage::new([("todos".to_owned(), "[]".to_owned())]);
        assert_eq!(storage.load().unwrap()["todos"], "[]");
        storage.write(StorageChange::Set("theme".into(), "dark".into()));
        storage.write(StorageChange::Remove("todos".into()));
        assert_eq!(
            storage.entries().into_iter().collect::<Vec<_>>(),
            [("theme".to_owned(), "dark".to_owned())]
        );
    }
}
