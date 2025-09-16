
cross-all: cross-aarch64 cross-arm cross-i686 \
	cross-powerpc64le cross-s390x cross-x86_64

define run-cross
	cross +1.63 build -v --target $(1)
	cross +1.63 doc --target $(1)
	cross +1.63 test --target $(1) || true
	cross build -v --target $(1)
	cross doc --target $(1)
	cross test --target $(1) || true
	cross +nightly build -v --target $(1)
	cross +nightly doc --target $(1)
	cross +nightly test --target $(1) || true
endef

cross-aarch64:
	$(call run-cross, aarch64-unknown-linux-gnu)
	$(call run-cross, aarch64-unknown-linux-musl)

cross-arm:
	$(call run-cross, arm-unknown-linux-gnueabi)
	$(call run-cross, arm-unknown-linux-gnueabihf)
	$(call run-cross, arm-unknown-linux-musleabi)
	$(call run-cross, arm-unknown-linux-musleabihf)

cross-armv7:
	$(call run-cross, armv7-unknown-linux-gnueabi)
	$(call run-cross, armv7-unknown-linux-gnueabihf)
	$(call run-cross, armv7-unknown-linux-musleabi)
	$(call run-cross, armv7-unknown-linux-musleabihf)

cross-i686:
	$(call run-cross, i686-unknown-linux-gnu)
	$(call run-cross, i686-unknown-linux-musl)

cross-mips:
# See: https://github.com/rust-lang/rust/issues/115218
	$(call run-cross, mips-unknown-linux-gnu)
	$(call run-cross, mips-unknown-linux-musl)

cross-mips64:
	$(call run-cross, mips64-unknown-linux-gnuabi64)
	$(call run-cross, mips64-unknown-linux-muslabi64)

cross-mips64el:
	$(call run-cross, mips64el-unknown-linux-gnuabi64)
	$(call run-cross, mips64el-unknown-linux-muslabi64)

cross-mipsel:
	$(call run-cross, mipsel-unknown-linux-gnu)
	$(call run-cross, mipsel-unknown-linux-musl)

cross-powerpc64:
	$(call run-cross, powerpc64-unknown-linux-gnu)

cross-powerpc64le:
	$(call run-cross, powerpc64le-unknown-linux-gnu)

cross-riscv64gc:
	$(call run-cross, riscv64gc-unknown-linux-gnu)

cross-s390x:
	$(call run-cross, s390x-unknown-linux-gnu)

cross-x86_64:
	$(call run-cross, x86_64-unknown-linux-gnu)
	$(call run-cross, x86_64-unknown-linux-musl)

