PACKAGES = tari_crypto taiji_core tari_utilities taiji_comms
doc:
	$(foreach p,$(PACKAGES),cargo rustdoc -p $(p) -- --html-in-header meta/assets/rustdoc-include-js-header.html;)
doc-internal:
	$(foreach p,$(PACKAGES),cargo rustdoc -p $(p) -- --html-in-header docs/assets/rustdoc-include-js-header.html --document-private-items;)
