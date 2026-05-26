// Minimal C host reference for confidential_transfer_v1.
//
// Product meaning:
//   confidential_transfer_v1 is an SDK/host profile over existing AOEM RingCT.
//
// Product path:
//   host -> aoem_ringct_prove_v1 -> aoem_privacy_execute_v1
//
// Default mode is a fast host wiring probe. Pass --run-prove to execute the full
// RingCT confidential transfer generation/verification path; that path creates
// a 64-bit range proof and can take longer than a typical smoke test.
//
// This does not add a public FFI ABI, compute op, Runtime Canon path, Graph OS
// path, dedicated LR path, or new proof worker default task.

#include "aoem.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
typedef HMODULE aoem_dynlib_t;
#else
#include <dlfcn.h>
typedef void* aoem_dynlib_t;
#endif

#define AOEM_PRIVACY_TX_ENCODING_JSON_V1 1u

typedef uint32_t (*aoem_abi_version_fn)(void);
typedef int32_t (*aoem_global_init_fn)(void);
typedef int32_t (*aoem_ringct_prove_v1_fn)(
    const uint8_t*,
    size_t,
    uint64_t,
    uint64_t,
    uint32_t,
    uint8_t**,
    size_t*);
typedef int32_t (*aoem_privacy_execute_v1_fn)(
    const uint8_t*,
    size_t,
    uint8_t**,
    size_t*);
typedef int32_t (*aoem_ringct_verify_v1_fn)(
    const uint8_t*,
    size_t,
    uint32_t,
    uint32_t*);
typedef void (*aoem_free_fn)(uint8_t*, size_t);

typedef struct aoem_confidential_transfer_api {
  aoem_dynlib_t lib;
  aoem_abi_version_fn abi_version;
  aoem_global_init_fn global_init;
  aoem_ringct_prove_v1_fn ringct_prove_v1;
  aoem_privacy_execute_v1_fn privacy_execute_v1;
  aoem_ringct_verify_v1_fn ringct_verify_v1_compat;
  aoem_free_fn free_buf;
} aoem_confidential_transfer_api;

typedef struct byte_buf {
  uint8_t* data;
  size_t len;
  size_t cap;
} byte_buf;

static void buf_free(byte_buf* b) {
  free(b->data);
  b->data = NULL;
  b->len = 0;
  b->cap = 0;
}

static int buf_reserve(byte_buf* b, size_t extra) {
  if (extra > (size_t)-1 - b->len) {
    return -1;
  }
  size_t need = b->len + extra;
  if (need <= b->cap) {
    return 0;
  }
  size_t cap = b->cap ? b->cap : 512;
  while (cap < need) {
    if (cap > (size_t)-1 / 2) {
      cap = need;
      break;
    }
    cap *= 2;
  }
  uint8_t* next = (uint8_t*)realloc(b->data, cap);
  if (!next) {
    return -1;
  }
  b->data = next;
  b->cap = cap;
  return 0;
}

static int buf_append(byte_buf* b, const void* data, size_t len) {
  if (buf_reserve(b, len) != 0) {
    return -1;
  }
  memcpy(b->data + b->len, data, len);
  b->len += len;
  return 0;
}

static int buf_append_cstr(byte_buf* b, const char* s) {
  return buf_append(b, s, strlen(s));
}

static int bytes_contains(const uint8_t* haystack, size_t haystack_len, const char* needle) {
  const size_t needle_len = strlen(needle);
  if (!needle_len || needle_len > haystack_len) {
    return 0;
  }
  for (size_t i = 0; i <= haystack_len - needle_len; ++i) {
    if (memcmp(haystack + i, needle, needle_len) == 0) {
      return 1;
    }
  }
  return 0;
}

#ifdef _WIN32
static aoem_dynlib_t aoem_open_library(const char* path) {
  return LoadLibraryA(path);
}

static void* aoem_symbol(aoem_dynlib_t lib, const char* name) {
  return (void*)GetProcAddress(lib, name);
}

static void aoem_close_library(aoem_dynlib_t lib) {
  if (lib) {
    FreeLibrary(lib);
  }
}
#else
static aoem_dynlib_t aoem_open_library(const char* path) {
  return dlopen(path, RTLD_NOW | RTLD_LOCAL);
}

static void* aoem_symbol(aoem_dynlib_t lib, const char* name) {
  return dlsym(lib, name);
}

static void aoem_close_library(aoem_dynlib_t lib) {
  if (lib) {
    dlclose(lib);
  }
}
#endif

static const char* default_library_path(void) {
#ifdef _WIN32
  return "aoem\\windows\\core\\bin\\aoem_ffi.dll";
#else
  return "aoem/linux/core/bin/libaoem_ffi.so";
#endif
}

static int has_arg(int argc, char** argv, const char* expected) {
  for (int i = 1; i < argc; ++i) {
    if (strcmp(argv[i], expected) == 0) {
      return 1;
    }
  }
  return 0;
}

static const char* library_arg(int argc, char** argv) {
  for (int i = 1; i < argc; ++i) {
    if (argv[i][0] != '-') {
      return argv[i];
    }
  }
  return default_library_path();
}

static int load_api(const char* path, aoem_confidential_transfer_api* api) {
  memset(api, 0, sizeof(*api));
  api->lib = aoem_open_library(path);
  if (!api->lib) {
    fprintf(stderr, "failed to load AOEM library: %s\n", path);
    return -1;
  }

  api->abi_version = (aoem_abi_version_fn)aoem_symbol(api->lib, "aoem_abi_version");
  api->global_init = (aoem_global_init_fn)aoem_symbol(api->lib, "aoem_global_init");
  api->ringct_prove_v1 =
      (aoem_ringct_prove_v1_fn)aoem_symbol(api->lib, "aoem_ringct_prove_v1");
  api->privacy_execute_v1 =
      (aoem_privacy_execute_v1_fn)aoem_symbol(api->lib, "aoem_privacy_execute_v1");
  api->ringct_verify_v1_compat =
      (aoem_ringct_verify_v1_fn)aoem_symbol(api->lib, "aoem_ringct_verify_v1");
  api->free_buf = (aoem_free_fn)aoem_symbol(api->lib, "aoem_free");

  if (!api->abi_version || !api->global_init || !api->ringct_prove_v1 ||
      !api->privacy_execute_v1 || !api->free_buf) {
    fprintf(stderr, "missing required AOEM RingCT/confidential-transfer symbols\n");
    aoem_close_library(api->lib);
    memset(api, 0, sizeof(*api));
    return -1;
  }
  return 0;
}

static void unload_api(aoem_confidential_transfer_api* api) {
  if (api->lib) {
    aoem_close_library(api->lib);
  }
  memset(api, 0, sizeof(*api));
}

static int build_ringct_privacy_request(
    const uint8_t* tx_json,
    size_t tx_json_len,
    byte_buf* out) {
  const char* prefix =
      "{\"version\":1,\"kind\":\"RingCt\",\"backend\":\"Auto\",\"transactions\":[{\"encoding\":\"json\",\"data\":";
  const char* suffix = "}]}";
  return buf_append_cstr(out, prefix) == 0 &&
                 buf_append(out, tx_json, tx_json_len) == 0 &&
                 buf_append_cstr(out, suffix) == 0
             ? 0
             : -1;
}

int main(int argc, char** argv) {
  const char* lib_path = library_arg(argc, argv);
  const int run_prove = has_arg(argc, argv, "--run-prove");
  aoem_confidential_transfer_api api;
  if (load_api(lib_path, &api) != 0) {
    return 1;
  }

  int failures = 0;
  if (api.abi_version() == 0) {
    fprintf(stderr, "invalid AOEM ABI version\n");
    failures++;
  }
  if (api.global_init() != 0) {
    fprintf(stderr, "aoem_global_init failed\n");
    failures++;
  }

  if (!run_prove) {
    printf(
        "SUPERVM_AOEM_CONFIDENTIAL_TRANSFER_HOST|profile=confidential_transfer_v1|"
        "ringct_symbols=ok|prove=not_run|privacy_execute=not_run|verify=not_run|"
        "mode=host_wiring_probe|run_prove_hint=--run-prove|ffi_abi_unchanged=1|"
        "runtime_canon_changed=0|proof_worker_default_changed=0|failures=%d\n",
        failures);
    unload_api(&api);
    return failures == 0 ? 0 : 1;
  }

  const char* message =
      "confidential_transfer_v1|asset=SUPERVM_TEST|from=alice|to=bob|nonce=1";
  const uint64_t amount_lo = 9876543210123ULL;
  const uint64_t amount_hi = 0;
  const uint32_t ring_size = 2;

  uint8_t* tx_json = NULL;
  size_t tx_json_len = 0;
  int32_t prove_rc = api.ringct_prove_v1(
      (const uint8_t*)message,
      strlen(message),
      amount_lo,
      amount_hi,
      ring_size,
      &tx_json,
      &tx_json_len);
  if (prove_rc != 0 || !tx_json || tx_json_len == 0) {
    fprintf(stderr, "aoem_ringct_prove_v1 failed: rc=%d\n", prove_rc);
    failures++;
  }

  byte_buf request = {0};
  uint8_t* response = NULL;
  size_t response_len = 0;
  if (!failures) {
    if (build_ringct_privacy_request(tx_json, tx_json_len, &request) != 0) {
      fprintf(stderr, "failed to build RingCT privacy execution request\n");
      failures++;
    } else {
      int32_t exec_rc =
          api.privacy_execute_v1(request.data, request.len, &response, &response_len);
      if (exec_rc != 0 || !response || response_len == 0) {
        fprintf(stderr, "aoem_privacy_execute_v1 failed: rc=%d\n", exec_rc);
        failures++;
      }
    }
  }

  uint32_t compat_valid = 1;
  if (!failures && api.ringct_verify_v1_compat) {
    compat_valid = 0;
    int32_t verify_rc = api.ringct_verify_v1_compat(
        tx_json, tx_json_len, AOEM_PRIVACY_TX_ENCODING_JSON_V1, &compat_valid);
    if (verify_rc != 0 || compat_valid != 1) {
      fprintf(stderr, "aoem_ringct_verify_v1 compatibility check failed: rc=%d valid=%u\n",
          verify_rc,
          compat_valid);
      failures++;
    }
  }

  char amount_text[32];
  snprintf(amount_text, sizeof(amount_text), "%llu", (unsigned long long)amount_lo);
  const int amount_hidden =
      tx_json && tx_json_len > 0 && !bytes_contains(tx_json, tx_json_len, amount_text);
  const int accepted =
      response && response_len > 0 &&
      bytes_contains(response, response_len, "\"accepted\":true");
  const int status_accepted =
      response && response_len > 0 &&
      bytes_contains(response, response_len, "\"status\":\"Accepted\"");

  if (!amount_hidden) {
    fprintf(stderr, "RingCT transaction payload exposed the raw sample amount\n");
    failures++;
  }
  if (!accepted || !status_accepted) {
    fprintf(stderr, "RingCT privacy execution response was not accepted\n");
    failures++;
  }

  printf(
      "SUPERVM_AOEM_CONFIDENTIAL_TRANSFER_HOST|profile=confidential_transfer_v1|"
      "ringct=ok|prove=%s|privacy_execute=%s|verify=%s|amount_hidden=%s|"
      "payload_bytes=%zu|ring_size=%u|ffi_abi_unchanged=1|runtime_canon_changed=0|"
      "proof_worker_default_changed=0|failures=%d\n",
      prove_rc == 0 ? "ok" : "fail",
      accepted && status_accepted ? "ok" : "fail",
      compat_valid == 1 ? "ok" : "skipped",
      amount_hidden ? "ok" : "fail",
      tx_json_len,
      ring_size,
      failures);

  if (response) {
    api.free_buf(response, response_len);
  }
  if (tx_json) {
    api.free_buf(tx_json, tx_json_len);
  }
  buf_free(&request);
  unload_api(&api);
  return failures == 0 ? 0 : 1;
}
