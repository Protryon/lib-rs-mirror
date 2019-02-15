STYLES=style/public/index.css style/public/search.css
CACHE_FILES=data/crate_data.db data/users.db data/2019.rmpz

all: website

website: download-caches data/index/1 styles
	cd front_end && cargo run --release --bin website
	cd style && npm start

download-caches: $(CACHE_FILES)

$(CACHE_FILES):
	if [ ! -d data/data.tar.xz -a -f data.tar.xz ]; then mv data.tar.xz data/; fi
	if [ ! -f data/data.tar.xz ]; then curl --fail --output data/data.tar.xz https://crates.rs/data/data.tar.xz; fi

	cd data; unxz < data.tar.xz | tar xv

styles: $(STYLES)

$(STYLES): style/node_modules/.bin/gulp
	cd style && npm run build

style/package.json:
	git submodule update --init --recursive

style/node_modules/.bin/gulp: style/package.json
	@echo Installing Sass
	cd style && npm install
	touch $@

data/index/1:
	@echo Getting crates index
	git submodule update --init

.PHONY: all download-caches styles clean clean-cache

clean:
	rm -rf style/public/*.css
	git submodule update --init --recursive
	git submodule sync

clean-cache:
	rm data/*rmpz data/users.db
