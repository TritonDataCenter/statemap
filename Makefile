#
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.
#

#
# Copyright (c) 2015, Joyent, Inc.
#

#
# Tools
#
JSL		 = ./deps/javascriptlint/build/install/jsl
JSSTYLE		 = ./deps/jsstyle/jsstyle
# md2man-roff can be found at <https://github.com/sunaku/md2man>.
MD2MAN          := md2man-roff
NODE	 	 = node
NPM		 = npm

#
# Tool configuration
#
JSL_CONF_NODE	 = ./tools/jsl.node.conf
JSSTYLE_FLAGS	 = -f ./tools/jsstyle.conf

#
# Paths and input files
#
JS_FILES	:= \
	$(wildcard ./*.js ./lib/*.js ./test/*.js) \
	bin/statemap

JSL_FILES_NODE	 = $(JS_FILES)
JSSTYLE_FILES	 = $(JS_FILES)

include Makefile.defs

all:
	$(NPM) install

#
# No doubt other tests under test/ should be included by this, but they're not
# well enough documented at this point to incorporate.
#
test: all
	$(CATEST) -a
	@echo tests okay

#
# Manual pages are committed to the repo so that most developers don't have to
# install the tools required to build them, but the target exists here so that
# you can rebuild them automatically when desired.
#
.PHONY: manpages
manpages: $(MAN_OUTPAGES)

$(MAN_OUTPAGES): $(MAN_OUTDIR)/%.1: $(MAN_SOURCEDIR)/%.md
	$(MD2MAN) $^ > $@

include Makefile.deps
include Makefile.targ
