all: website

website: caches data/index/1 styles
	cd front_end && cargo run --release --bin website
	cd style && npm start

caches: data/cache.db data/crates.db data/github.db data/crate_meta.db data/users.db

styles: style/public/index.css

style/public/index.css: style/node_modules/.bin/gulp
	cd style && npm run build

style/node_modules/.bin/gulp:
	@echo Installing Sass
	cd style && npm install
	touch $@

data/index/1:
	@echo Getting crates index
	git submodule update

%.db: %.db.xz
	@echo Uncompressing $@
	-rm -f $@
	unxz -vk $<

data/cache.db.xz data/github.db.xz data/crate_meta.db.xz data/users.db.xz data/crates.db:
	@echo Downloading $@
	curl --fail --output $@ https://crates.rs/$@

.PHONY: all caches styles clean

clean:
	rm -rf style/public/*.css
