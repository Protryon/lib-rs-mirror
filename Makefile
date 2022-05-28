STYLES=style/public/index.css style/public/search.css
CACHE_FILES=data/crate_data.db data/users.db data/2019.rmpz

all: website

website: data/index/1 styles Cargo.lock
	cd server && cargo run --release
	cd style && npm start

Cargo.lock: Cargo.toml
	rm Cargo.lock; git submodule update --init --recursive
	cargo update

backup_data:
	rsync -Praz librs:/var/lib/crates-server/crate_data.db data/crate_data.db;

backup:
	rsync -Praz librs:/var/lib/crates-server/ data/ --exclude=tarballs --exclude=git --exclude=index --exclude=event_log.db --exclude='.*' --exclude='*.txt' --exclude=data
	rsync -Praz librs:/var/lib/crates-server/ data/ --exclude=tarballs --exclude='*.mpbr' --exclude=git --exclude=index --exclude=event_log.db --exclude='.*' --exclude='*.txt' --exclude=data

$(CACHE_FILES):
	if [ ! -d data/data.tar.xz -a -f data.tar.xz ]; then mv data.tar.xz data/; fi
	if [ ! -f data/data.tar.xz ]; then curl --fail --output data/data.tar.xz https://lib.rs/data/data.tar.xz; fi

	cd data; unxz < data.tar.xz | tar xv
	touch $@

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
	rm -rf style/public/*.css Cargo.lock
	git submodule update --init --recursive
	git submodule sync

