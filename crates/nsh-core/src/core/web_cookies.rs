//! A web view's `getCookies` in the core: each call is a request of this
//! engine, sent to the platform ([`Command::WebViewRequest`]) and answered
//! once, by the platform's reply ([`crate::EventKind::WebViewReply`]), its
//! deadline, or the web view going away.

use std::{rc::Rc, time::Duration};

use rustc_hash::FxHashMap;

use super::{Core, Kind};
use crate::{
    Command, JsonValue, NodeId,
    element::{
        JsError, Task, codes,
        task::{self, Action, AnswerSlot},
    },
    webview::{WebCookie, WebViewOperation, WebViewRequest, cookies},
};

pub(crate) type CookieAnswer = AnswerSlot<Result<Vec<WebCookie>, JsError>>;

struct Pending {
    node: NodeId,
    answer: CookieAnswer,
    _deadline: Task<()>,
}

#[derive(Default)]
pub(super) struct CookieRequests {
    pending: FxHashMap<u64, Pending>,
}

impl CookieRequests {
    /// Whether `request` is an in-flight `getCookies` request owned by this core.
    pub(super) fn tracks(&self, request: u64) -> bool {
        self.pending.contains_key(&request)
    }
}

impl<M: crate::TextEngine> Core<M> {
    /// `ref.getCookies(url, { timeout })`: settles JS promise `call` with
    /// `WebViewCookie[]` or rejects.
    pub(super) fn start_js_web_cookies(
        &mut self,
        node: NodeId,
        call: u32,
        url: String,
        timeout: Duration,
    ) {
        use crate::element::{Answer, IntoThrown, ToJs};
        let (answer, slot) = Answer::new();
        self.start_web_cookies(node, url, timeout, slot);
        let executor = Rc::downgrade(&self.components.executor);
        task::spawn_raw(
            &self.components.executor,
            crate::ROOT_ID,
            Box::pin(async move {
                let result = answer
                    .await
                    .and_then(|list| {
                        serde_json::to_value(list).map_err(|e| {
                            JsError::Error(format!("failed to serialize cookies: {e}"))
                                .with_code(codes::WEBVIEW_ABORTED)
                        })
                    })
                    .map(|json| Box::new(json) as Box<dyn ToJs>)
                    .map_err(IntoThrown::into_thrown);
                if let Some(executor) = executor.upgrade() {
                    executor.act(Action::Return(call, result));
                }
            }),
        );
    }

    pub(super) fn start_web_cookies(
        &mut self,
        node: NodeId,
        url: String,
        timeout: Duration,
        answer: CookieAnswer,
    ) {
        if Rc::strong_count(&answer) == 1 {
            return;
        }
        let node = self.native_id(node);
        let refused = if !self.components.environment.web_cookies {
            Some(
                JsError::Error("web views read no cookies here".into())
                    .with_code(codes::UNAVAILABLE),
            )
        } else if self
            .key(node)
            .ok()
            .is_none_or(|key| self.nodes[key as usize].kind != Kind::WebView)
        {
            Some(
                JsError::Error("the ref names no web view".into())
                    .with_code(codes::WEBVIEW_ABORTED),
            )
        } else if !self.access.allows_webviews() {
            Some(JsError::access_denied(
                "the app's capabilities do not grant webview",
                "webview",
                "",
            ))
        } else if !cookies::is_http_url(&url) {
            Some(JsError::Type(
                "getCookies expects an http or https URL".into(),
            ))
        } else if !self.access.allows_webview(&url) {
            Some(JsError::access_denied(
                "the app's capabilities do not grant webview access to this URL",
                "webview",
                &url,
            ))
        } else if self.components.web_cookies.pending.len() >= cookies::MAX_PENDING {
            Some(
                JsError::Error("WebView cookie requests exceed the pending limit".into())
                    .with_code(codes::QUOTA_EXCEEDED),
            )
        } else {
            None
        };
        if let Some(error) = refused {
            answer.borrow_mut().deliver(Err(error));
            return;
        }
        let request = self.alloc_webview_request();
        let deadline = task::act_after(
            &self.components.executor,
            crate::ROOT_ID,
            timeout,
            Action::WebCookiesTimeout(request),
        );
        self.components.web_cookies.pending.insert(
            request,
            Pending {
                node,
                answer,
                _deadline: deadline,
            },
        );
        self.commands.push((
            node,
            Command::WebViewRequest(WebViewRequest {
                id: request,
                operation: WebViewOperation::GetCookies { url },
            }),
        ));
    }

    pub(super) fn answer_web_cookies(&mut self, node: NodeId, request: u64, reply: &JsonValue) {
        if !self
            .components
            .web_cookies
            .pending
            .get(&request)
            .is_some_and(|p| p.node == node)
        {
            return;
        }
        let pending = self
            .components
            .web_cookies
            .pending
            .remove(&request)
            .expect("checked request");
        let result = match reply.get("ok").and_then(JsonValue::as_bool) {
            Some(true) => match reply.get("value") {
                Some(value) => cookies::decode_reply(value),
                None => Err(JsError::Error("the WebView answered no cookies".into())
                    .with_code(codes::WEBVIEW_ABORTED)),
            },
            _ => {
                let message = reply
                    .get("error")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("the WebView failed to read cookies");
                Err(JsError::Error(message.into()).with_code(codes::WEBVIEW_ABORTED))
            }
        };
        pending.answer.borrow_mut().deliver(result);
    }

    pub(super) fn timeout_web_cookies(&mut self, request: u64) {
        if let Some(pending) = self.components.web_cookies.pending.remove(&request) {
            pending.answer.borrow_mut().deliver(Err(JsError::Error(
                "the WebView's cookie store did not answer in time".into(),
            )
            .with_code(codes::TIMED_OUT)));
        }
    }

    pub(super) fn cancel_web_cookies(&mut self, node: NodeId) {
        if self.components.web_cookies.pending.is_empty() {
            return;
        }
        self.components.web_cookies.pending.retain(|_, pending| {
            if pending.node != node {
                return true;
            }
            pending
                .answer
                .borrow_mut()
                .deliver(Err(JsError::Error("the web view went away".into())
                    .with_code(codes::WEBVIEW_ABORTED)));
            false
        });
    }
}
