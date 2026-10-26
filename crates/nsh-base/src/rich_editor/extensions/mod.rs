//! Optional editor behavior on the kernel's plugins: undo history
//! ([`history`], after `prosemirror-history`), key bindings ([`keymap`],
//! after `prosemirror-keymap`), input rules ([`input_rules`], after
//! `prosemirror-inputrules`) and a starter set ([`starter`]): a schema of
//! common nodes and marks (`prosemirror-schema-basic` with lists) and the
//! bindings and rules for it. None of it is built in: an editor takes the
//! pieces it wants, or its own.

pub mod history;
pub mod input_rules;
pub mod keymap;
pub mod markdown;
pub mod starter;

pub use history::{
    HistoryOptions, HistoryState, close_history, history, history_key, history_state,
    is_history_transaction, redo, redo_depth, redo_no_scroll, undo, undo_depth, undo_no_scroll,
};
pub use input_rules::{
    GetAttrs, InCode, InputMatch, InputRule, InputRuleHandler, InputRulesState, JoinPredicate,
    ellipsis, em_dash, input_rules, mark_input_rule, smart_quotes, textblock_type_input_rule,
    undo_input_rule, wrapping_input_rule,
};
pub use keymap::{
    Keymap, KeymapError, KeymapOptions, keymap, keymap_plugin, keymap_with, normalize_key_name,
};
