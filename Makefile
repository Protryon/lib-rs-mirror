all: website

website: caches data/index/1 styles
	cd front_end && cargo run --release --bin website
	cd style && npm start

caches: data/crate_data.db

data/crate_data.db:
	if [ ! -d data/data.tar.xz -a -f data.tar.xz ]; then mv data.tar.xz data/; fi
	if [ ! -f data/data.tar.xz ]; then curl --fail --output data/data.tar.xz https://crates.rs/data/data.tar.xz; fi

	cd data; unxz < data.tar.xz | tar xv

styles: style/public/index.css

style/public/index.css: style/node_modules/.bin/gulp
	cd style && npm run build

style/node_modules/.bin/gulp:
	@echo Installing Sass
	cd style && npm install
	touch $@

data/index/1:
	@echo Getting crates index
	git submodule update --init

.PHONY: all caches styles clean

clean:
	rm -rf style/public/*.css
