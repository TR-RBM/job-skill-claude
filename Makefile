PREFIX ?= $(HOME)/.local
CLAUDE_HOME ?= $(HOME)/.claude
JOB_CONFIG_DIR ?= $(if $(XDG_CONFIG_HOME),$(XDG_CONFIG_HOME),$(HOME)/.config)/job
CARGO ?= cargo
INSTALL ?= install

bindir = $(PREFIX)/bin
skilldir = $(CLAUDE_HOME)/skills/job
policy = $(JOB_CONFIG_DIR)/policy.json

.PHONY: build check install install-policy uninstall

build:
	$(CARGO) build --release

check:
	$(CARGO) fmt --check
	$(CARGO) clippy --quiet --all-targets -- -D warnings
	$(CARGO) test --quiet

install: build
	$(INSTALL) -d $(DESTDIR)$(bindir) $(DESTDIR)$(skilldir)
	$(INSTALL) -m 0755 target/release/job-hook-claude $(DESTDIR)$(bindir)/job-hook-claude
	$(INSTALL) -m 0644 skill/job/SKILL.md $(DESTDIR)$(skilldir)/SKILL.md
	@if [ -e "$(DESTDIR)$(policy)" ]; then \
		echo "command policy: $(DESTDIR)$(policy) exists and is left as it is; it is yours to edit"; \
	else \
		echo "command policy: there is none at $(DESTDIR)$(policy), so job forbids nothing"; \
		echo "command policy: 'make install-policy' installs the default from policy/policy.json there"; \
	fi

install-policy:
	@if [ -e "$(DESTDIR)$(policy)" ]; then \
		echo "command policy: $(DESTDIR)$(policy) exists and is not replaced"; \
	else \
		$(INSTALL) -d "$(DESTDIR)$(JOB_CONFIG_DIR)" && \
		$(INSTALL) -m 0644 policy/policy.json "$(DESTDIR)$(policy)" && \
		echo "command policy: installed $(DESTDIR)$(policy); it is yours to edit, and 'job policy --show' validates it"; \
	fi

uninstall:
	rm -f $(DESTDIR)$(bindir)/job-hook-claude $(DESTDIR)$(skilldir)/SKILL.md
	-rmdir $(DESTDIR)$(skilldir)
	@echo "command policy: $(DESTDIR)$(policy) is not removed; it belongs to you"
