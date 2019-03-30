doc:
	cargo rustdoc -p crypto --open -- --html-in-header meta/assets/rustdoc-include-katex-header.html
doc-internal:
	cargo rustdoc -p crypto -- --html-in-header docs/assets/rustdoc-include-katex-header.html --document-private-items

