use arborium_clojure;
use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add Clojure language support.
pub fn add_clojure_support() {
    LanguageRegistry::singleton().register(
        "clojure",
        &LanguageConfig::new(
            "clojure",
            arborium_clojure::language().into(),
            vec![],
            CLOJURE_HIGHLIGHTS_QUERY,
            arborium_clojure::INJECTIONS_QUERY,
            arborium_clojure::LOCALS_QUERY,
        ),
    );
}

/// Custom highlights query for Clojure.
///
/// The upstream arborium_clojure query only covers literals (numbers, strings,
/// booleans, keywords, comments). Because Clojure is homoiconic, special forms
/// such as `defn`, `let`, `fn`, and `if` are indistinguishable from ordinary
/// function calls at the grammar level — both are a `list_lit` whose first
/// child is a `sym_lit`. A `#match?` predicate on the symbol text is required
/// to recognise them as keywords.
///
/// Capture names are mapped to the names recognised by gpui-component's
/// `HIGHLIGHT_NAMES` (`keyword`, `function`, `type`, `string.special.symbol`,
/// etc.).
const CLOJURE_HIGHLIGHTS_QUERY: &str = r##"
;; ──────────────── Literals ────────────────

(num_lit) @number

[
  (str_lit)
  (char_lit)
] @string

(regex_lit) @string.regex

(bool_lit) @boolean

(nil_lit) @constant

;; Keywords are symbols prefixed with `:` — highlight as string.special.symbol
;; so they stand out from regular identifiers without using the type colour.
(kwd_lit) @string.special.symbol

;; ──────────────── Comments ────────────────

(comment) @comment

;; Discard expressions (#_ form) are treated as comments.
(dis_expr) @comment

;; ──────────────── Metadata ────────────────

(meta_lit     ["^"]  @attribute)
(old_meta_lit ["#^"] @attribute)

;; ──────────────── Reader macros / quoting operators ────────────────

(quoting_lit          ["'"]  @operator)
(syn_quoting_lit      ["`"]  @operator)
(unquoting_lit        ["~"]  @operator)
(unquote_splicing_lit ["~@"] @operator)
(derefing_lit         ["@"]  @operator)
(var_quoting_lit      ["#'"] @operator)

;; ──────────────── Definition name extraction ────────────────

;; Capture the name (second sym_lit child) of defining forms as @function.
;; The `@_kind` capture is used only for the predicate and produces no highlight.
((list_lit . (sym_lit) @_kind . (sym_lit) @function)
 (#match? @_kind "^(defn-?|defmacro|defmulti|defmethod)$"))

;; Capture the name of type/record/protocol definitions as @type.
((list_lit . (sym_lit) @_kind . (sym_lit) @type)
 (#match? @_kind "^(defrecord|deftype|definterface|defstruct|defprotocol)$"))

;; Capture the namespace name in ns / in-ns forms as @type.
((list_lit . (sym_lit) @_kind . (sym_lit) @type)
 (#match? @_kind "^(ns|in-ns)$"))

;; ──────────────── Special forms and core macros ────────────────

;; This rule must appear AFTER the name-extraction rules above so that
;; `defn`, `defrecord`, etc. are captured both as @keyword (the symbol
;; itself) and produce a @function / @type capture for the name that follows.
((list_lit . (sym_lit) @keyword)
 (#match? @keyword
  "^(def|defn|defn-|defmacro|defmulti|defmethod|defonce|defrecord|deftype|definterface|defprotocol|defstruct|ns|in-ns|let|letfn|if|if-let|if-not|if-some|when|when-let|when-not|when-some|cond|condp|case|and|or|not|do|fn|loop|recur|throw|try|catch|finally|new|set!|quote|var|declare|require|use|import|refer|refer-clojure|load|load-file|binding|doto|with-open|with-local-vars|with-redefs|locking|monitor-enter|monitor-exit|proxy|extend|extend-type|extend-protocol|reify|->|->>|as->|some->|some->>|cond->|cond->>|dotimes|doseq|for|while|when-first|lazy-seq|lazy-cat|delay|dosync|io!)$"))

;; ──────────────── Function calls ────────────────

;; Any other first-position symbol in a list is treated as a function call.
;; This rule is last so that the keyword matches above take priority for
;; special forms.
(list_lit . (sym_lit) @function)
"##;

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_clojure_support_registers_language() {
        super::add_clojure_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("clojure")
                .is_some()
        );
    }
}
