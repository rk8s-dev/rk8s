/* crc_fast library C/C++ API - Copyright 2025 Don MacAskill */
/* This header is auto-generated. Do not edit directly. */

#ifndef CRC_FAST_H
#define CRC_FAST_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * The supported CRC algorithms
 */
typedef enum CrcFastAlgorithm {
  Crc32Aixm,
  Crc32Autosar,
  Crc32Base91D,
  Crc32Bzip2,
  Crc32CdRomEdc,
  Crc32Cksum,
  Crc32Iscsi,
  Crc32IsoHdlc,
  Crc32Jamcrc,
  Crc32Mef,
  Crc32Mpeg2,
  Crc32Xfer,
  Crc64Ecma182,
  Crc64GoIso,
  Crc64Ms,
  Crc64Nvme,
  Crc64Redis,
  Crc64We,
  Crc64Xz,
} CrcFastAlgorithm;

/**
 * Represents a CRC Digest, which is used to compute CRC checksums.
 *
 * The `Digest` struct maintains the state of the CRC computation, including
 * the current state, the amount of data processed, the CRC parameters, and
 * the calculator function used to perform the CRC calculation.
 */
typedef struct CrcFastDigest CrcFastDigest;

/**
 * A handle to the Digest object
 */
typedef struct CrcFastDigestHandle {
  struct CrcFastDigest *_0;
} CrcFastDigestHandle;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Creates a new Digest to compute CRC checksums using algorithm
 */
struct CrcFastDigestHandle *crc_fast_digest_new(enum CrcFastAlgorithm algorithm);

/**
 * Updates the Digest with data
 */
void crc_fast_digest_update(struct CrcFastDigestHandle *handle, const char *data, uintptr_t len);

/**
 * Calculates the CRC checksum for data that's been written to the Digest
 */
uint64_t crc_fast_digest_finalize(struct CrcFastDigestHandle *handle);

/**
 * Free the Digest resources without finalizing
 */
void crc_fast_digest_free(struct CrcFastDigestHandle *handle);

/**
 * Reset the Digest state
 */
void crc_fast_digest_reset(struct CrcFastDigestHandle *handle);

/**
 * Finalize and reset the Digest in one operation
 */
uint64_t crc_fast_digest_finalize_reset(struct CrcFastDigestHandle *handle);

/**
 * Combine two Digest checksums
 */
void crc_fast_digest_combine(struct CrcFastDigestHandle *handle1,
                             struct CrcFastDigestHandle *handle2);

/**
 * Gets the amount of data processed by the Digest so far
 */
uint64_t crc_fast_digest_get_amount(struct CrcFastDigestHandle *handle);

/**
 * Helper method to calculate a CRC checksum directly for a string using algorithm
 */
uint64_t crc_fast_checksum(enum CrcFastAlgorithm algorithm, const char *data, uintptr_t len);

/**
 * Helper method to just calculate a CRC checksum directly for a file using algorithm
 */
uint64_t crc_fast_checksum_file(enum CrcFastAlgorithm algorithm,
                                const uint8_t *path_ptr,
                                uintptr_t path_len);

/**
 * Combine two CRC checksums using algorithm
 */
uint64_t crc_fast_checksum_combine(enum CrcFastAlgorithm algorithm,
                                   uint64_t checksum1,
                                   uint64_t checksum2,
                                   uint64_t checksum2_len);

/**
 * Gets the target build properties (CPU architecture and fine-tuning parameters) for this algorithm
 */
const char *crc_fast_get_calculator_target(enum CrcFastAlgorithm algorithm);

/**
 * Gets the version of this library
 */
const char *crc_fast_get_version(void);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* CRC_FAST_H */
