//! Per-engine correlation and lifetime of awaited WebView evaluations.

use std::{rc::Rc, time::Duration};

use rustc_hash::FxHashMap;

use super::{Core, Kind};
use crate::{
    Command, EventPayload, NodeId, PayloadValue,
    element::{
        Task,
        task::{self, Action},
    },
    web_evaluation::{self, Slot},
};

struct Pending {
    node: NodeId,
    answer: Slot,
    _deadline: Task<()>,
}

#[derive(Default)]
pub(super) struct Evaluations {
    next: u32,
    pending: FxHashMap<u32, Pending>,
}

impl<M: crate::TextEngine> Core<M> {
    pub(super) fn start_js_web_evaluation(&mut self, node: NodeId, call: u32, script: String) {
        use crate::element::{Answer, IntoThrown, ToJs};
        let (answer, slot) = Answer::new();
        self.start_web_evaluation(node, script, slot);
        let executor = Rc::downgrade(&self.components.executor);
        task::spawn_raw(
            &self.components.executor,
            crate::ROOT_ID,
            Box::pin(async move {
                let result = answer
                    .await
                    .map(|value| Box::new(value) as Box<dyn ToJs>)
                    .map_err(IntoThrown::into_thrown);
                if let Some(executor) = executor.upgrade() {
                    executor.act(Action::Return(call, result));
                }
            }),
        );
    }

    pub(super) fn start_web_evaluation(&mut self, node: NodeId, script: String, answer: Slot) {
        if Rc::strong_count(&answer) == 1 {
            return;
        }
        let node = self.native_id(node);
        let refused = if !self.components.environment.web_evaluation {
            Some(web_evaluation::error(
                "ERR_FEATURE_UNAVAILABLE_ON_PLATFORM",
                "WebView evaluation is available on iOS and Android",
            ))
        } else if self
            .key(node)
            .ok()
            .is_none_or(|key| self.nodes[key as usize].kind != Kind::WebView)
        {
            Some(web_evaluation::error(
                "ERR_NSH_WEBVIEW_DISPOSED",
                "the ref does not name a live WebView",
            ))
        } else if script.len() > web_evaluation::MAX_RESULT_BYTES
            || self.components.web_evaluations.pending.len() >= web_evaluation::MAX_PENDING
        {
            Some(web_evaluation::error(
                "ERR_NSH_QUOTA_EXCEEDED",
                "WebView evaluation exceeds the script or pending request limit",
            ))
        } else {
            None
        };
        if let Some(error) = refused {
            answer.borrow_mut().deliver(Err(error));
            return;
        }
        let requests = &mut self.components.web_evaluations;
        let Some(request) = requests.next.checked_add(1) else {
            answer.borrow_mut().deliver(Err(web_evaluation::error(
                "ERR_NSH_QUOTA_EXCEEDED",
                "WebView request ids exhausted",
            )));
            return;
        };
        requests.next = request;
        let deadline = task::act_after(
            &self.components.executor,
            crate::ROOT_ID,
            Duration::from_millis(web_evaluation::TIMEOUT_MS),
            Action::WebEvaluationTimeout(request),
        );
        requests.pending.insert(
            request,
            Pending {
                node,
                answer,
                _deadline: deadline,
            },
        );
        self.commands
            .push((node, Command::EvaluateJavaScript { request, script }));
    }

    pub(super) fn answer_web_evaluation(&mut self, node: NodeId, payload: &EventPayload) {
        let EventPayload::Map(fields) = payload else {
            return;
        };
        let Some(request) = fields.iter().find_map(|(key, value)| match value {
            PayloadValue::Number(id)
                if *key == "id"
                    && id.is_finite()
                    && *id >= 1.0
                    && *id <= u32::MAX as f64
                    && id.fract() == 0.0 =>
            {
                Some(*id as u32)
            }
            _ => None,
        }) else {
            return;
        };
        if !self
            .components
            .web_evaluations
            .pending
            .get(&request)
            .is_some_and(|p| p.node == node)
        {
            return;
        }
        let pending = self
            .components
            .web_evaluations
            .pending
            .remove(&request)
            .expect("checked request");
        let result = fields
            .iter()
            .find_map(|(key, value)| match value {
                PayloadValue::Str(data) if *key == "data" => Some(web_evaluation::decode(data)),
                _ => None,
            })
            .unwrap_or_else(|| {
                Err(web_evaluation::error(
                    "ERR_NSH_WEBVIEW_RESULT",
                    "missing native evaluation result",
                ))
            });
        pending.answer.borrow_mut().deliver(result);
    }

    pub(super) fn timeout_web_evaluation(&mut self, request: u32) {
        if let Some(pending) = self.components.web_evaluations.pending.remove(&request) {
            pending
                .answer
                .borrow_mut()
                .deliver(Err(web_evaluation::error(
                    "ETIMEDOUT",
                    "WebView evaluation exceeded 30 seconds",
                )));
            self.commands
                .push((pending.node, Command::CancelJavaScript { request }));
        }
    }

    pub(super) fn forget_web_evaluations(&mut self) {
        let dropped: Vec<_> = self
            .components
            .web_evaluations
            .pending
            .iter()
            .filter_map(|(&id, p)| (Rc::strong_count(&p.answer) == 1).then_some(id))
            .collect();
        for request in dropped {
            if let Some(pending) = self.components.web_evaluations.pending.remove(&request) {
                self.commands
                    .push((pending.node, Command::CancelJavaScript { request }));
            }
        }
    }

    pub(super) fn cancel_web_evaluations(&mut self, node: NodeId) {
        if self.components.web_evaluations.pending.is_empty() {
            return;
        }
        self.components
            .web_evaluations
            .pending
            .retain(|_, pending| {
                if pending.node != node {
                    return true;
                }
                pending
                    .answer
                    .borrow_mut()
                    .deliver(Err(web_evaluation::error(
                        "ERR_NSH_WEBVIEW_DISPOSED",
                        "the WebView was disposed",
                    )));
                false
            });
    }
}
