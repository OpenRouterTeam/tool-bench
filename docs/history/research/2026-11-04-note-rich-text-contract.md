# CRM note rich-text parser research (2026-11-04)

Scope: dependency compatibility, exact Rust parser/tree APIs, and the boundary between
decoded DOM text and rendered text. This is implementation research, not the behavioral
test plan or adjudication/reporting specification.

## Dependency recommendation

For the smallest implementation using an existing static tree, use this **matched pair**:

```toml
html5ever = "0.39.0"
markup5ever_rcdom = "0.39.0"
```

The latter resolves the published `0.39.0+unofficial` release; `+unofficial` is SemVer
build metadata, not a prerelease. Cargo ignores build metadata in requirements.
These are normal compatible-version requirements, not exact `=` pins. [1][2][3]

- Registry metadata checked on **November 4, 2026** lists `html5ever 0.40.1`
  (September 14, 2026) and `markup5ever_rcdom 0.39.0+unofficial` (May 5, 2026)
  as their newest non-yanked stable releases. [1][2]
- **Do not combine those two newest releases.** Published rcdom `0.39.0+unofficial`
  depends on `html5ever ^0.39` and `markup5ever ^0.39`; html5ever `0.40.1` uses
  `markup5ever ^0.40`. Its `TreeSink` interface is consequently from a different
  crate version, so that rcdom is not a sink for the `0.40` parser.
  This incompatibility follows from the published dependencies, not matching
  numbers by convention. [4][5]
- Two direct dependencies suffice: html5ever re-exports `QualName`, the name
  macros, and `tendril`; no additional direct entity-decoder, selector, browser,
  `markup5ever`, or `tendril` dependency is required for the APIs below. [6]
- This is economical in implementation effort, **not the minimum transitive graph**:
  rcdom also depends unconditionally on `xml5ever ^0.39`, `tendril ^0.5`, and
  `markup5ever` with its `serde` feature. There are no rcdom feature flags for
  removing that baggage. A custom html5ever `TreeSink` or narrow `TokenSink`
  could avoid rcdom, but requires additional implementation work. [4][7][8]

Important suitability caveat: rcdom's own published documentation explicitly disclaims
production readiness, maintenance/support for arbitrary malicious input, and fuzzing
of such inputs. It is a testing-oriented static tree, not a supported browser DOM.
Using it for bounded offline benchmark-note checking is an engineering tradeoff,
not a security guarantee. Apply input/traversal resource limits; do not reuse the
snippet as a general-purpose untrusted-HTML service. [9]

## Exact APIs for the compatible 0.39 pair

The public parser entry points are:

```rust
pub fn parse_document<Sink: TreeSink>(
    sink: Sink,
    opts: ParseOpts,
) -> Parser<Sink>;

pub fn parse_fragment<Sink: TreeSink>(
    sink: Sink,
    opts: ParseOpts,
    context_name: QualName,
    context_attrs: Vec<Attribute>,
    context_element_allows_scripting: bool,
) -> Parser<Sink>;
```

**`parse_fragment` takes five arguments in 0.39**, including the final boolean;
older four-argument examples are not this API. Both return a `Parser` implementing
`TendrilSink`; importing `html5ever::tendril::TendrilSink` enables `.one(input)`,
whose output is `RcDom` when the sink is `RcDom`. Parsing a Rust `&str` needs
neither `.from_utf8()` nor a separate tendril conversion. [10]

Prefer fragment parsing with an HTML `div` context for note markup. Fragment
parsing is context-sensitive; its rcdom document contains a synthetic `html`
root, **not a synthesized `div` context wrapper**. Full-document parsing instead
performs document/head/body construction. Do not confuse synthesized structure
with permission for source-level document/head markup. [11][12]

Relevant tree API:

- `RcDom { document: Handle, errors: RefCell<Vec<Cow<'static, str>>>,
  quirks_mode: Cell<QuirksMode> }`; `Handle = Rc<Node>`.
- `Node { parent: Cell<Option<WeakHandle>>, children: RefCell<Vec<Handle>>,
  data: NodeData }`.
- `NodeData::Text { contents: RefCell<StrTendril> }`.
- `NodeData::Element { name: QualName, attrs: RefCell<Vec<Attribute>>,
  template_contents: RefCell<Option<Handle>>,
  mathml_annotation_xml_integration_point: bool }`.
- Other variants are `Document`, `Doctype { name, public_id, system_id }`,
  `Comment { contents }`, and `ProcessingInstruction { target, contents }`.
  In particular, template content is stored separately from ordinary children.
  Borrow `&node.data`, `node.children.borrow()`, `attrs.borrow()`, and
  `contents.borrow()` rather than attempting to move data out of an `Rc`. [13]

### Small source-checked API sketch

This demonstrates parsing and a deliberately narrow DOM walk. **It is not a
standalone acceptance checker**: source-token validation and parse-error handling
must precede accepting its candidate (see the recovery caveat below).
The walker emits block boundaries, not browser-exact whitespace; normalization
and comparison with the user's literal text remain the caller's contract.

```rust
use html5ever::tendril::TendrilSink;
use html5ever::{ParseOpts, QualName, parse_document, parse_fragment};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

const HTML_NS: &str = "http://www.w3.org/1999/xhtml";

fn note_fragment(input: &str) -> RcDom {
    let mut options = ParseOpts::default();
    options.tree_builder.scripting_enabled = false;
    parse_fragment(
        RcDom::default(),
        options,
        QualName::new(None, HTML_NS.into(), "div".into()),
        Vec::new(),
        false,
    )
    .one(input)
}

fn whole_document(input: &str) -> RcDom {
    parse_document(RcDom::default(), ParseOpts::default()).one(input)
}

fn collect_candidate(node: &Handle, output: &mut String) -> Result<(), &'static str> {
    let boundary = match &node.data {
        NodeData::Document => false,
        NodeData::Text { contents } => {
            output.push_str(&contents.borrow());
            return Ok(());
        }
        NodeData::Element { name, attrs, .. } => {
            if name.ns.as_ref() != HTML_NS || !attrs.borrow().is_empty() {
                return Err("unsupported rendering");
            }
            match name.local.as_ref() {
                "html" | "span" | "b" | "strong" | "i" | "em" | "u" => false,
                "div" | "p" | "br" => true,
                _ => return Err("unsupported rendering"),
            }
        }
        _ => return Err("unsupported rendering"),
    };
    if boundary {
        output.push('\n');
    }
    for child in node.children.borrow().iter() {
        collect_candidate(child, output)?;
    }
    if boundary {
        output.push('\n');
    }
    Ok(())
}
```

`html` here permits only the parser-created fragment root after a source-level
gate; it is not a recommendation to accept user-supplied `<html>` tags.
`collect_candidate` is for `note_fragment`, not `whole_document`: the latter's
ordinary synthetic `head` is intentionally not in this walker's allowlist.
Discard the entire candidate on `Err`, including text already accumulated.
Check `dom.errors.borrow()` explicitly: `.one()` returns a repaired DOM, not
`Result<RcDom, _>`. The recursive walk is kept small for illustration; a deployed
checker should bound depth or use an explicit stack. [10][11][13]

`TreeBuilderOpts::scripting_enabled` defaults to `true`; setting it to `false`
changes parsing behavior, especially for `noscript`, but does not sanitize the
markup or prove its browser behavior. The fragment boolean separately controls
the context element's tokenizer state (notably a `noscript` context).
Neither setting makes `noscript` supported by this sketch. [11]

## Why DOM text is not proof of rendered text

1. **Text nodes are not visibility evidence.** DOM descendant text content is
   the concatenation of descendant `Text` data in tree order. It does not consult
   CSS, layout, visibility, or scripting. Technically `Document.textContent` is
   null; recursively walking all text under a document is a custom extraction,
   not that getter. [14]
2. **Head and active content can contribute misleading text.** Default HTML
   rendering hides `head`, `title`, `script`, `style`, and `template`, among
   others. Script/style bodies still contain source text; finding the user's
   requested words there does not establish note-body text. Do not strip such
   nodes and then call a matching remainder proof of the original note. [15]
3. **CSS and state require a renderer.** `hidden` normally suppresses rendering
   (including special `until-found` handling), while CSS can override relevant
   defaults. `display`, `visibility`, whitespace processing, and text transforms
   affect the rendered-text algorithm. Attribute-free markup can still inherit
   the CRM host's CSS; the restricted checker therefore needs an explicit
   ordinary-formatting/host-styling assumption, not a universal visibility
   claim. [15][16][17]
4. **Even `innerText` is not an unconditional visual oracle.** Its algorithm uses
   rendered boxes and computed styles when the root is rendered, but falls back
   to descendant text content when the root is not rendered or the user agent
   does not support CSS. A detached/hidden-root `innerText` result is therefore
   not a substitute for the proposed proof. [17]
5. **Entities and block spacing are separate from visibility.** HTML tokenization
   resolves character references into text; do not decode a second time.
   `<br>` and block layout supply boundaries not present as DOM text characters,
   so raw text-node concatenation can fuse words across blocks. Conversely,
   inserting spaces around every inline element changes ordinary inline text.
   CSS whitespace rules are not equivalent to collapsing every Unicode
   whitespace character: e.g. `&nbsp;` becomes U+00A0, not an ordinary CSS
   collapsible space. Any deliberately broader whitespace equivalence is a
   benchmark contract, not browser-exact rendering. [17][18][19]

### Critical parser-recovery caveat

A DOM allowlist alone does **not** prove that the original source used only
allowed markup. HTML tree construction can ignore tags, discard attributes,
repair nesting, and relocate content. For example, an `in body` fragment can
ignore a `head` start tag; attributes on end tags do not become element
attributes. Thus a repaired tree can look supported after unsupported source
syntax has disappeared. [12][20]

For a conservative proof, validate the original token stream against the narrow
attribute-free formatting vocabulary as well as validating the resulting DOM;
do not rely solely on the repaired tree or a regex tag stripper. Html5ever
already exposes `TokenSink` and tag tokens with `kind`, `name`, `self_closing`,
`attrs`, and `had_duplicate_attributes`, so this need not add a lexer dependency.
A preflight restricted to ordinary formatting can stop at the first unsupported
token rather than interpreting raw-text, foreign-content, or active subtrees.
Preserve parse errors as evidence of unsupported/recovery behavior rather than
assuming a returned DOM means valid or harmless HTML. This is a recommended
implementation boundary, not an additional adjudication policy. [8][10][20][21]

**Bottom line:** decoded text from an explicitly supported, attribute-free,
passive formatting fragment can support a narrowly defined text-equivalence
check. Hidden/active/head/template/foreign/unknown markup or attributes must not
be converted into proof merely because their extracted text matches. Surface
unsupported rendering instead of guessing; a general browser renderer is not
required for that conservative boundary. This recommendation follows from the
API and standards limitations above.

## Primary sources

All package metadata and published-source links were checked on 2026-11-04.
All 24 source URLs returned successfully, and the Rust sketch passed syntax
parsing with the installed `rustfmt`. No dependencies were installed and the
sketch was not compiled or type-checked; API compatibility was checked against
the versioned published source.

1. [crates.io: html5ever versions, publication timestamps, yanked state, Rust version](https://crates.io/api/v1/crates/html5ever)
2. [crates.io: markup5ever_rcdom versions and publication timestamps](https://crates.io/api/v1/crates/markup5ever_rcdom)
3. [Cargo Reference: version requirements and build metadata](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#version-requirement-syntax)
4. [crates.io: rcdom 0.39.0+unofficial dependency requirements](https://crates.io/api/v1/crates/markup5ever_rcdom/0.39.0%2Bunofficial/dependencies)
5. [crates.io: html5ever 0.40.1 dependency requirements](https://crates.io/api/v1/crates/html5ever/0.40.1/dependencies)
6. [html5ever 0.39.0 source: public re-exports](https://docs.rs/html5ever/0.39.0/src/html5ever/lib.rs.html#16-17)
7. [rcdom 0.39.0+unofficial: feature flags](https://docs.rs/crate/markup5ever_rcdom/0.39.0+unofficial/features)
8. [html5ever 0.39.0: TokenSink](https://docs.rs/html5ever/0.39.0/html5ever/tokenizer/trait.TokenSink.html)
9. [rcdom 0.39.0+unofficial: support and production-readiness warning](https://docs.rs/markup5ever_rcdom/0.39.0+unofficial/markup5ever_rcdom/)
10. [html5ever 0.39.0 driver source: entry points and TendrilSink output](https://docs.rs/html5ever/0.39.0/src/html5ever/driver.rs.html#33-138)
11. [html5ever 0.39.0 tree-builder source: options defaults, fragment root and context tokenizer state](https://docs.rs/html5ever/0.39.0/src/html5ever/tree_builder/mod.rs.html#71-265)
12. [WHATWG HTML: fragment parsing](https://html.spec.whatwg.org/multipage/parsing.html#parsing-html-fragments)
13. [rcdom 0.39.0+unofficial source: NodeData, Node, Handle and RcDom](https://docs.rs/markup5ever_rcdom/0.39.0+unofficial/src/markup5ever_rcdom/lib.rs.html#72-124)
14. [WHATWG DOM: descendant text content](https://dom.spec.whatwg.org/#concept-descendant-text-content) and [get text content](https://dom.spec.whatwg.org/#get-text-content)
15. [WHATWG HTML: default hidden-element rendering](https://html.spec.whatwg.org/multipage/rendering.html#hidden-elements)
16. [WHATWG HTML: hidden attribute](https://html.spec.whatwg.org/multipage/interaction.html#the-hidden-attribute)
17. [WHATWG HTML: innerText getter and rendered-text collection](https://html.spec.whatwg.org/multipage/dom.html#the-innertext-idl-attribute)
18. [WHATWG HTML: named character reference state](https://html.spec.whatwg.org/multipage/parsing.html#named-character-reference-state) and [named reference table](https://html.spec.whatwg.org/multipage/named-characters.html#named-character-references)
19. [CSS Text Module Level 3: whitespace processing](https://drafts.csswg.org/css-text-3/#white-space-rules)
20. [WHATWG HTML: in-body tree-construction rules](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-inbody) and [end-tag-with-attributes parse error](https://html.spec.whatwg.org/multipage/parsing.html#parse-error-end-tag-with-attributes)
21. [html5ever 0.39.0: tag-token fields](https://docs.rs/html5ever/0.39.0/html5ever/tokenizer/struct.Tag.html)

## Implementation decision

The benchmark uses **html5ever 0.40.1's tokenizer only**, avoiding rcdom and its XML
dependency entirely. The actual published crate was downloaded with `cargo info`;
its `examples/tokenize.rs` and `tokenizer/interface.rs` supplied the compiled
`BufferQueue`, `Tokenizer`, `TokenSink`, `Tag`, and parse-error interfaces. These
APIs are also documented in the [versioned tokenizer module](https://docs.rs/html5ever/0.40.1/html5ever/tokenizer/index.html)
and [published example source](https://docs.rs/crate/html5ever/0.40.1/source/examples/tokenize.rs).

Rather than approximating general DOM rendering, the checker validates a deliberately
small balanced passive-fragment grammar in the original token stream. Anything
outside that supported grammar becomes an explicit unresolved assessment. The
behavioral contract, resource bounds and declared whitespace equivalence are in
`docs/contracts.md`; successful tokenization alone is not treated as proof of
visibility or semantic correctness.
