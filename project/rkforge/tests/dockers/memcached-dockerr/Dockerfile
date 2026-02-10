#
# NOTE: THIS DOCKERFILE IS GENERATED VIA "apply-templates.sh"
#
# PLEASE DO NOT EDIT IT DIRECTLY.
#

FROM debian:trixie-slim

# add our user and group first to make sure their IDs get assigned consistently, regardless of whatever dependencies get added
RUN set -eux; \
	groupadd --system --gid 11211 memcache; \
	useradd --system --gid memcache --uid 11211 memcache

# ensure SASL's "libplain.so" is installed as per https://github.com/memcached/memcached/wiki/SASLHowto
RUN set -eux; \
	apt-get update; \
	apt-get install -y --no-install-recommends \
		libsasl2-modules \
	; \
	rm -rf /var/lib/apt/lists/*

ENV MEMCACHED_VERSION 1.6.40
ENV MEMCACHED_URL https://memcached.org/files/memcached-1.6.40.tar.gz
ENV MEMCACHED_SHA1 f2513db7079ee4c6558eb11fabb55e1adf1fdf38

RUN set -eux; \
	\
	savedAptMark="$(apt-mark showmanual)"; \
	apt-get update; \
	apt-get install -y --no-install-recommends \
		ca-certificates \
		dpkg-dev \
		gcc \
		libc6-dev \
		libevent-dev \
		libio-socket-ssl-perl \
		libsasl2-dev \
		libssl-dev \
		make \
		perl \
		wget \
	; \
	rm -rf /var/lib/apt/lists/*; \
	\
	wget -O memcached.tar.gz "$MEMCACHED_URL"; \
	echo "$MEMCACHED_SHA1  memcached.tar.gz" | sha1sum -c -; \
	mkdir -p /usr/src/memcached; \
	tar -xzf memcached.tar.gz -C /usr/src/memcached --strip-components=1; \
	rm memcached.tar.gz; \
	\
	cd /usr/src/memcached; \
	\
	gnuArch="$(dpkg-architecture --query DEB_BUILD_GNU_TYPE)"; \
# https://github.com/memcached/memcached/issues/1220#issuecomment-2770251664: on arm32, we need to override the upstream alignment check (which fails to detect the need for alignment on arm32v6+ on our hardware for some reason, which then causes us to fail the tests 😭)
	case "$gnuArch" in \
		arm-*abihf) export ac_cv_c_alignment=need ;; \
	esac; \
	./configure \
		--build="$gnuArch" \
		--enable-extstore \
		--enable-proxy \
		--enable-sasl \
		--enable-sasl-pwdb \
		--enable-tls \
	; \
	nproc="$(nproc)"; \
	make -j "$nproc"; \
	\
# see https://github.com/docker-library/memcached/pull/54#issuecomment-562797748 and https://bugs.debian.org/927461 for why we have to munge openssl.cnf
	sed -i.bak 's/SECLEVEL=2/SECLEVEL=1/g' /etc/ssl/openssl.cnf; \
# try the tests in parallel first, but many of them are resource-intensive, so fall back to serial
	make test PARALLEL="$nproc" || make test; \
	mv /etc/ssl/openssl.cnf.bak /etc/ssl/openssl.cnf; \
	\
	make install; \
	\
	cd /; \
	rm -rf /usr/src/memcached; \
	\
	apt-mark auto '.*' > /dev/null; \
	apt-mark manual $savedAptMark > /dev/null; \
	find /usr/local -type f -executable -exec ldd '{}' ';' \
		| awk '/=>/ { so = $(NF-1); if (index(so, "/usr/local/") == 1) { next }; gsub("^/(usr/)?", "", so); printf "*%s\n", so }' \
		| sort -u \
		| xargs -r dpkg-query --search \
		| cut -d: -f1 \
		| sort -u \
		| xargs -r apt-mark manual \
	; \
	apt-get purge -y --auto-remove -o APT::AutoRemove::RecommendsImportant=false; \
	\
	memcached -V

COPY docker-entrypoint.sh /usr/local/bin/
RUN ln -s usr/local/bin/docker-entrypoint.sh /entrypoint.sh # backwards compat
ENTRYPOINT ["docker-entrypoint.sh"]

USER memcache
EXPOSE 11211
CMD ["memcached"]
