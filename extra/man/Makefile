# Man page generation
.PHONY: all clean install

SCDOC = scdoc
MANDIR = /usr/local/share/man

all: neoism.1 neoism.5 neoism-bindings.5

neoism.1: neoism.1.scd
	$(SCDOC) < $< > $@

neoism.5: neoism.5.scd
	$(SCDOC) < $< > $@

neoism-bindings.5: neoism-bindings.5.scd
	$(SCDOC) < $< > $@

install: all
	mkdir -p $(DESTDIR)$(MANDIR)/man1 $(DESTDIR)$(MANDIR)/man5
	install -m644 neoism.1 $(DESTDIR)$(MANDIR)/man1/neoism.1
	install -m644 neoism.5 $(DESTDIR)$(MANDIR)/man5/neoism.5
	install -m644 neoism-bindings.5 $(DESTDIR)$(MANDIR)/man5/neoism-bindings.5

clean:
	rm -f neoism.1 neoism.5 neoism-bindings.5
