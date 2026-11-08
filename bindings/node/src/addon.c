/* Addon N-API de geoda para Node (HU-064). Es fino: convierte tipos de JavaScript y llama a la
 * C-API (ABI 1.x). La API amigable (clases, validacion de tipos) esta en `index.js`.
 *
 * Una columna es un `external` con un `Handle`; `release()` pone `col` en NULL, asi que liberar
 * a mano y luego por el recolector no libera dos veces. */
#define NAPI_VERSION 8
#include <node_api.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "geoda.h"

typedef struct {
  struct GeodaColumn *col;
} Handle;

static const char *category_of(int code) {
  switch (code) {
    case GEODA_ERR_INVALID_ARGUMENT: return "INVALID_ARGUMENT";
    case GEODA_ERR_TYPE_MISMATCH: return "TYPE_MISMATCH";
    case GEODA_ERR_LENGTH_MISMATCH: return "LENGTH_MISMATCH";
    case GEODA_ERR_OUT_OF_BOUNDS: return "OUT_OF_BOUNDS";
    case GEODA_ERR_UNSUPPORTED: return "UNSUPPORTED";
    case GEODA_ERR_ALLOC: return "ALLOC";
    case GEODA_ERR_FFI: return "FFI";
    case GEODA_ERR_PANIC: return "PANIC";
    default: return "INTERNAL";
  }
}

/* Lanza un Error de JavaScript con `code` y `category`. Siempre devuelve NULL. */
static napi_value throw_code(napi_env env, int code, const char *message) {
  char text[1024];
  snprintf(text, sizeof text, "%s: %s", category_of(code), message);
  napi_value msg, error, number, category;
  napi_create_string_utf8(env, text, NAPI_AUTO_LENGTH, &msg);
  napi_create_error(env, NULL, msg, &error);
  napi_create_int32(env, code, &number);
  napi_create_string_utf8(env, category_of(code), NAPI_AUTO_LENGTH, &category);
  napi_set_named_property(env, error, "code", number);
  napi_set_named_property(env, error, "category", category);
  napi_throw(env, error);
  return NULL;
}

static napi_value throw_geoda(napi_env env, int code) {
  return throw_code(env, code, geoda_last_error_message());
}

/* Error de uso del addon (tipos mal pasados desde JavaScript): TypeError. */
static napi_value throw_type(napi_env env, const char *message) {
  napi_throw_type_error(env, NULL, message);
  return NULL;
}

#define CHECK_NAPI(env, call)                                         \
  do {                                                                \
    napi_status status_ = (call);                                     \
    if (status_ != napi_ok) {                                         \
      const napi_extended_error_info *info_ = NULL;                   \
      napi_get_last_error_info((env), &info_);                        \
      return throw_type((env), info_ && info_->error_message ? info_->error_message : "N-API"); \
    }                                                                 \
  } while (0)

static void finalize_handle(napi_env env, void *data, void *hint) {
  (void)env;
  (void)hint;
  Handle *handle = (Handle *)data;
  if (handle->col != NULL) geoda_column_free(handle->col);
  free(handle);
}

static napi_value wrap_column(napi_env env, struct GeodaColumn *col) {
  Handle *handle = (Handle *)malloc(sizeof(Handle));
  if (handle == NULL) {
    geoda_column_free(col);
    return throw_code(env, GEODA_ERR_ALLOC, "sin memoria para el handle");
  }
  handle->col = col;
  napi_value external;
  if (napi_create_external(env, handle, finalize_handle, NULL, &external) != napi_ok) {
    geoda_column_free(col);
    free(handle);
    return throw_type(env, "no se pudo crear el external");
  }
  return external;
}

static struct GeodaColumn *column_of(napi_env env, napi_value value) {
  void *data = NULL;
  if (napi_get_value_external(env, value, &data) != napi_ok || data == NULL) {
    throw_type(env, "se esperaba una columna de geoda");
    return NULL;
  }
  struct GeodaColumn *col = ((Handle *)data)->col;
  if (col == NULL) {
    napi_throw_error(env, NULL, "la columna ya se libero");
    return NULL;
  }
  return col;
}

/* Datos de un typed array del tipo esperado. */
static int typed_array(napi_env env, napi_value value, napi_typedarray_type want, void **data,
                       size_t *length) {
  bool is_typed = false;
  napi_is_typedarray(env, value, &is_typed);
  napi_typedarray_type type;
  if (!is_typed || napi_get_typedarray_info(env, value, &type, length, data, NULL, NULL) != napi_ok ||
      type != want) {
    throw_type(env, "tipo de arreglo incorrecto");
    return 0;
  }
  return 1;
}

static int is_undefined(napi_env env, napi_value value) {
  napi_valuetype type;
  napi_typeof(env, value, &type);
  return type == napi_undefined || type == napi_null;
}

/* fromTyped(kind, array, validity?) -> external. kind: 0 f64, 1 i32, 2 i64, 3 bool. */
static napi_value from_typed(napi_env env, napi_callback_info info) {
  size_t argc = 3;
  napi_value argv[3];
  CHECK_NAPI(env, napi_get_cb_info(env, info, &argc, argv, NULL, NULL));
  int32_t kind;
  CHECK_NAPI(env, napi_get_value_int32(env, argv[0], &kind));
  static const napi_typedarray_type types[] = {napi_float64_array, napi_int32_array,
                                               napi_bigint64_array, napi_uint8_array};
  if (kind < 0 || kind > 3) return throw_type(env, "kind invalido");
  void *values = NULL;
  size_t len = 0;
  if (!typed_array(env, argv[1], types[kind], &values, &len)) return NULL;
  const uint8_t *validity = NULL;
  if (argc > 2 && !is_undefined(env, argv[2])) {
    void *bytes = NULL;
    size_t vlen = 0;
    if (!typed_array(env, argv[2], napi_uint8_array, &bytes, &vlen)) return NULL;
    if (vlen != len) return throw_code(env, GEODA_ERR_LENGTH_MISMATCH, "validity mide distinto que values");
    validity = (const uint8_t *)bytes;
  }
  struct GeodaColumn *out = NULL;
  int code;
  switch (kind) {
    case 0: code = geoda_column_from_f64((const double *)values, len, validity, &out); break;
    case 1: code = geoda_column_from_i32((const int32_t *)values, len, validity, &out); break;
    case 2: code = geoda_column_from_i64((const int64_t *)values, len, validity, &out); break;
    default: code = geoda_column_from_bool((const uint8_t *)values, len, validity, &out); break;
  }
  if (code != GEODA_OK) return throw_geoda(env, code);
  return wrap_column(env, out);
}

/* fromUtf8(data: Uint8Array, offsets: Int32Array, validity?) -> external. */
static napi_value from_utf8(napi_env env, napi_callback_info info) {
  size_t argc = 3;
  napi_value argv[3];
  CHECK_NAPI(env, napi_get_cb_info(env, info, &argc, argv, NULL, NULL));
  void *data = NULL, *offsets = NULL;
  size_t data_len = 0, offsets_len = 0;
  if (!typed_array(env, argv[0], napi_uint8_array, &data, &data_len)) return NULL;
  if (!typed_array(env, argv[1], napi_int32_array, &offsets, &offsets_len)) return NULL;
  if (offsets_len == 0) return throw_code(env, GEODA_ERR_INVALID_ARGUMENT, "offsets vacio");
  size_t len = offsets_len - 1;
  const uint8_t *validity = NULL;
  if (argc > 2 && !is_undefined(env, argv[2])) {
    void *bytes = NULL;
    size_t vlen = 0;
    if (!typed_array(env, argv[2], napi_uint8_array, &bytes, &vlen)) return NULL;
    if (vlen != len) return throw_code(env, GEODA_ERR_LENGTH_MISMATCH, "validity mide distinto que las filas");
    validity = (const uint8_t *)bytes;
  }
  struct GeodaColumn *out = NULL;
  int code = geoda_column_from_utf8((const uint8_t *)data, data_len, (const int32_t *)offsets, len,
                                    validity, &out);
  if (code != GEODA_OK) return throw_geoda(env, code);
  return wrap_column(env, out);
}

/* info(col) -> [len, nullCount, dtype] */
static napi_value column_info(napi_env env, napi_callback_info info) {
  size_t argc = 1;
  napi_value argv[1];
  CHECK_NAPI(env, napi_get_cb_info(env, info, &argc, argv, NULL, NULL));
  struct GeodaColumn *col = column_of(env, argv[0]);
  if (col == NULL) return NULL;
  int64_t len = geoda_column_len(col), nulls = geoda_column_null_count(col);
  int32_t dtype = geoda_column_dtype(col);
  if (len < 0) return throw_geoda(env, (int)len);
  if (nulls < 0) return throw_geoda(env, (int)nulls);
  if (dtype < 0) return throw_geoda(env, dtype);
  napi_value array, item;
  CHECK_NAPI(env, napi_create_array_with_length(env, 3, &array));
  napi_create_double(env, (double)len, &item);
  napi_set_element(env, array, 0, item);
  napi_create_double(env, (double)nulls, &item);
  napi_set_element(env, array, 1, item);
  napi_create_int32(env, dtype, &item);
  napi_set_element(env, array, 2, item);
  return array;
}

/* Crea un typed array de `len` elementos y devuelve su memoria en `*data`. */
static napi_value new_typed(napi_env env, napi_typedarray_type type, size_t elem, size_t len,
                            void **data) {
  napi_value buffer, array;
  CHECK_NAPI(env, napi_create_arraybuffer(env, elem * len, data, &buffer));
  CHECK_NAPI(env, napi_create_typedarray(env, type, len, buffer, 0, &array));
  return array;
}

/* copy(col, kind) -> typed array. kind: 0 f64, 1 i32, 2 i64, 3 bool. */
static napi_value copy_values(napi_env env, napi_callback_info info) {
  size_t argc = 2;
  napi_value argv[2];
  CHECK_NAPI(env, napi_get_cb_info(env, info, &argc, argv, NULL, NULL));
  struct GeodaColumn *col = column_of(env, argv[0]);
  if (col == NULL) return NULL;
  int32_t kind;
  CHECK_NAPI(env, napi_get_value_int32(env, argv[1], &kind));
  int64_t len = geoda_column_len(col);
  if (len < 0) return throw_geoda(env, (int)len);
  void *data = NULL;
  napi_value array;
  int code;
  switch (kind) {
    case 0:
      array = new_typed(env, napi_float64_array, 8, (size_t)len, &data);
      if (array == NULL) return NULL;
      code = geoda_column_copy_f64(col, (double *)data, (size_t)len);
      break;
    case 1:
      array = new_typed(env, napi_int32_array, 4, (size_t)len, &data);
      if (array == NULL) return NULL;
      code = geoda_column_copy_i32(col, (int32_t *)data, (size_t)len);
      break;
    case 2:
      array = new_typed(env, napi_bigint64_array, 8, (size_t)len, &data);
      if (array == NULL) return NULL;
      code = geoda_column_copy_i64(col, (int64_t *)data, (size_t)len);
      break;
    case 3:
      array = new_typed(env, napi_uint8_array, 1, (size_t)len, &data);
      if (array == NULL) return NULL;
      code = geoda_column_copy_bool(col, (uint8_t *)data, (size_t)len);
      break;
    default: return throw_type(env, "kind invalido");
  }
  if (code != GEODA_OK) return throw_geoda(env, code);
  return array;
}

static napi_value copy_validity(napi_env env, napi_callback_info info) {
  size_t argc = 1;
  napi_value argv[1];
  CHECK_NAPI(env, napi_get_cb_info(env, info, &argc, argv, NULL, NULL));
  struct GeodaColumn *col = column_of(env, argv[0]);
  if (col == NULL) return NULL;
  int64_t len = geoda_column_len(col);
  if (len < 0) return throw_geoda(env, (int)len);
  void *data = NULL;
  napi_value array = new_typed(env, napi_uint8_array, 1, (size_t)len, &data);
  if (array == NULL) return NULL;
  int code = geoda_column_copy_validity(col, (uint8_t *)data, (size_t)len);
  if (code != GEODA_OK) return throw_geoda(env, code);
  return array;
}

/* utf8(col) -> {data: Uint8Array, offsets: Int32Array} */
static napi_value copy_utf8(napi_env env, napi_callback_info info) {
  size_t argc = 1;
  napi_value argv[1];
  CHECK_NAPI(env, napi_get_cb_info(env, info, &argc, argv, NULL, NULL));
  struct GeodaColumn *col = column_of(env, argv[0]);
  if (col == NULL) return NULL;
  int64_t len = geoda_column_len(col);
  if (len < 0) return throw_geoda(env, (int)len);
  size_t total = 0;
  int code = geoda_column_utf8_data_len(col, &total);
  if (code != GEODA_OK) return throw_geoda(env, code);
  void *data = NULL, *offsets = NULL;
  napi_value data_array = new_typed(env, napi_uint8_array, 1, total, &data);
  if (data_array == NULL) return NULL;
  napi_value offsets_array = new_typed(env, napi_int32_array, 4, (size_t)len + 1, &offsets);
  if (offsets_array == NULL) return NULL;
  code = geoda_column_copy_utf8(col, (uint8_t *)data, total, (int32_t *)offsets, (size_t)len + 1);
  if (code != GEODA_OK) return throw_geoda(env, code);
  napi_value result;
  CHECK_NAPI(env, napi_create_object(env, &result));
  napi_set_named_property(env, result, "data", data_array);
  napi_set_named_property(env, result, "offsets", offsets_array);
  return result;
}

static napi_value release_column(napi_env env, napi_callback_info info) {
  size_t argc = 1;
  napi_value argv[1];
  CHECK_NAPI(env, napi_get_cb_info(env, info, &argc, argv, NULL, NULL));
  void *data = NULL;
  if (napi_get_value_external(env, argv[0], &data) != napi_ok || data == NULL) {
    return throw_type(env, "se esperaba una columna de geoda");
  }
  Handle *handle = (Handle *)data;
  if (handle->col != NULL) {
    geoda_column_free(handle->col);
    handle->col = NULL;
  }
  return NULL;
}

enum {
  OP_COUNT, OP_SUM, OP_MIN, OP_MAX, OP_MEAN, OP_VAR, OP_STD, OP_MEDIAN, OP_PERCENTILE
};

/* scalar(op, col, arg) -> number | bigint. Las sumas, minimos y maximos de columnas enteras
 * devuelven BigInt; `arg` es `ddof` (var, std) o `p` (percentile). */
static napi_value scalar(napi_env env, napi_callback_info info) {
  size_t argc = 3;
  napi_value argv[3];
  CHECK_NAPI(env, napi_get_cb_info(env, info, &argc, argv, NULL, NULL));
  int32_t op;
  CHECK_NAPI(env, napi_get_value_int32(env, argv[0], &op));
  struct GeodaColumn *col = column_of(env, argv[1]);
  if (col == NULL) return NULL;
  double arg = 0.0;
  if (argc > 2 && !is_undefined(env, argv[2])) CHECK_NAPI(env, napi_get_value_double(env, argv[2], &arg));
  int dtype = geoda_column_dtype(col);
  if (dtype < 0) return throw_geoda(env, dtype);
  int integer = dtype == GEODA_DTYPE_INT32 || dtype == GEODA_DTYPE_INT64;
  int64_t whole = 0;
  double real = 0.0;
  int code = GEODA_OK, gives_whole = 0;
  switch (op) {
    case OP_COUNT: code = geoda_column_count(col, &whole); gives_whole = 2; break;
    case OP_SUM:
      gives_whole = integer;
      code = integer ? geoda_column_sum_i64(col, 1, &whole) : geoda_column_sum_f64(col, 1, &real);
      break;
    case OP_MIN:
      gives_whole = integer;
      code = integer ? geoda_column_min_i64(col, 1, &whole) : geoda_column_min_f64(col, 1, &real);
      break;
    case OP_MAX:
      gives_whole = integer;
      code = integer ? geoda_column_max_i64(col, 1, &whole) : geoda_column_max_f64(col, 1, &real);
      break;
    case OP_MEAN: code = geoda_column_mean(col, 1, &real); break;
    case OP_VAR: code = geoda_column_var(col, (size_t)arg, 1, &real); break;
    case OP_STD: code = geoda_column_std(col, (size_t)arg, 1, &real); break;
    case OP_MEDIAN: code = geoda_column_median(col, &real); break;
    case OP_PERCENTILE: code = geoda_column_percentile(col, arg, &real); break;
    default: return throw_type(env, "op invalida");
  }
  if (code != GEODA_OK) return throw_geoda(env, code);
  napi_value result;
  if (gives_whole == 2) {
    CHECK_NAPI(env, napi_create_double(env, (double)whole, &result)); /* count: un Number */
  } else if (gives_whole == 1) {
    CHECK_NAPI(env, napi_create_bigint_int64(env, whole, &result));
  } else {
    CHECK_NAPI(env, napi_create_double(env, real, &result));
  }
  return result;
}

/* pair(op, a, b): op 0 = corr -> number; op 1 = editDistance -> external. */
static napi_value pair(napi_env env, napi_callback_info info) {
  size_t argc = 3;
  napi_value argv[3];
  CHECK_NAPI(env, napi_get_cb_info(env, info, &argc, argv, NULL, NULL));
  int32_t op;
  CHECK_NAPI(env, napi_get_value_int32(env, argv[0], &op));
  struct GeodaColumn *a = column_of(env, argv[1]);
  if (a == NULL) return NULL;
  struct GeodaColumn *b = column_of(env, argv[2]);
  if (b == NULL) return NULL;
  if (op == 0) {
    double out = 0.0;
    int code = geoda_column_corr(a, b, 1, &out);
    if (code != GEODA_OK) return throw_geoda(env, code);
    napi_value result;
    CHECK_NAPI(env, napi_create_double(env, out, &result));
    return result;
  }
  struct GeodaColumn *out = NULL;
  int code = geoda_column_edit_distance(a, b, &out);
  if (code != GEODA_OK) return throw_geoda(env, code);
  return wrap_column(env, out);
}

/* Arreglo de JavaScript de columnas -> `const GeodaColumn **` (a liberar con free). */
static const struct GeodaColumn **column_list(napi_env env, napi_value array, size_t *count) {
  bool is_array = false;
  napi_is_array(env, array, &is_array);
  uint32_t n = 0;
  if (!is_array || napi_get_array_length(env, array, &n) != napi_ok) {
    throw_type(env, "se esperaba un arreglo de columnas");
    return NULL;
  }
  const struct GeodaColumn **list = (const struct GeodaColumn **)calloc(n == 0 ? 1 : n, sizeof *list);
  if (list == NULL) {
    throw_code(env, GEODA_ERR_ALLOC, "sin memoria");
    return NULL;
  }
  for (uint32_t i = 0; i < n; i++) {
    napi_value item;
    napi_get_element(env, array, i, &item);
    list[i] = column_of(env, item);
    if (list[i] == NULL) {
      free(list);
      return NULL;
    }
  }
  *count = n;
  return list;
}

enum { LIST_MATMUL, LIST_SOLVE, LIST_DET, LIST_INVERSE };

/* list(op, listA, other): matmul(listA, listB) | solve(listA, b) | det(listA) | inverse(listA). */
static napi_value list_op(napi_env env, napi_callback_info info) {
  size_t argc = 3;
  napi_value argv[3];
  CHECK_NAPI(env, napi_get_cb_info(env, info, &argc, argv, NULL, NULL));
  int32_t op;
  CHECK_NAPI(env, napi_get_value_int32(env, argv[0], &op));
  size_t n_a = 0;
  const struct GeodaColumn **a = column_list(env, argv[1], &n_a);
  if (a == NULL) return NULL;
  struct GeodaColumn *out = NULL;
  int code = GEODA_OK;
  napi_value result = NULL;
  if (op == LIST_MATMUL) {
    size_t n_b = 0;
    const struct GeodaColumn **b = column_list(env, argv[2], &n_b);
    if (b == NULL) {
      free(a);
      return NULL;
    }
    code = geoda_matmul(a, n_a, b, n_b, &out);
    free(b);
  } else if (op == LIST_SOLVE) {
    struct GeodaColumn *rhs = column_of(env, argv[2]);
    if (rhs == NULL) {
      free(a);
      return NULL;
    }
    code = geoda_lu_solve(a, n_a, rhs, &out);
  } else if (op == LIST_DET) {
    double value = 0.0;
    code = geoda_det(a, n_a, &value);
    if (code == GEODA_OK) napi_create_double(env, value, &result);
  } else {
    code = geoda_inverse(a, n_a, &out);
  }
  free(a);
  if (code != GEODA_OK) return throw_geoda(env, code);
  return op == LIST_DET ? result : wrap_column(env, out);
}

/* kmeans(list, k, maxIter, tol, seed: bigint) -> {labels, centroids, inertia, iterations} */
static napi_value kmeans(napi_env env, napi_callback_info info) {
  size_t argc = 5;
  napi_value argv[5];
  CHECK_NAPI(env, napi_get_cb_info(env, info, &argc, argv, NULL, NULL));
  size_t n = 0;
  const struct GeodaColumn **list = column_list(env, argv[0], &n);
  if (list == NULL) return NULL;
  double k = 0, max_iter = 0, tol = 0;
  napi_get_value_double(env, argv[1], &k);
  napi_get_value_double(env, argv[2], &max_iter);
  napi_get_value_double(env, argv[3], &tol);
  uint64_t seed = 0;
  bool lossless = false;
  if (napi_get_value_bigint_uint64(env, argv[4], &seed, &lossless) != napi_ok) {
    free(list);
    return throw_type(env, "seed debe ser un BigInt");
  }
  struct GeodaColumn *labels = NULL, *centroids = NULL;
  double inertia = 0.0;
  int64_t iterations = 0;
  int code = geoda_kmeans(list, n, (size_t)k, (size_t)max_iter, tol, seed, &labels, &centroids,
                          &inertia, &iterations);
  free(list);
  if (code != GEODA_OK) return throw_geoda(env, code);
  napi_value result, value;
  CHECK_NAPI(env, napi_create_object(env, &result));
  value = wrap_column(env, labels);
  if (value == NULL) {
    geoda_column_free(centroids);
    return NULL;
  }
  napi_set_named_property(env, result, "labels", value);
  value = wrap_column(env, centroids);
  if (value == NULL) return NULL;
  napi_set_named_property(env, result, "centroids", value);
  napi_create_double(env, inertia, &value);
  napi_set_named_property(env, result, "inertia", value);
  napi_create_double(env, (double)iterations, &value);
  napi_set_named_property(env, result, "iterations", value);
  return result;
}

static napi_value abi_version(napi_env env, napi_callback_info info) {
  (void)info;
  napi_value result;
  CHECK_NAPI(env, napi_create_uint32(env, geoda_abi_version(), &result));
  return result;
}

#define EXPORT(name, fn)                                                   \
  do {                                                                     \
    napi_value function_;                                                  \
    if (napi_create_function(env, name, NAPI_AUTO_LENGTH, fn, NULL, &function_) != napi_ok) return NULL; \
    if (napi_set_named_property(env, exports, name, function_) != napi_ok) return NULL;                 \
  } while (0)

NAPI_MODULE_INIT() {
  EXPORT("fromTyped", from_typed);
  EXPORT("fromUtf8", from_utf8);
  EXPORT("info", column_info);
  EXPORT("copy", copy_values);
  EXPORT("validity", copy_validity);
  EXPORT("utf8", copy_utf8);
  EXPORT("release", release_column);
  EXPORT("scalar", scalar);
  EXPORT("pair", pair);
  EXPORT("list", list_op);
  EXPORT("kmeans", kmeans);
  EXPORT("abiVersion", abi_version);
  return exports;
}
