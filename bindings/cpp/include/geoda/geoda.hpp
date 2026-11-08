// geoda.hpp: envoltorio C++17 de solo cabeceras sobre la C-API de geoda (ABI 1.x).
//
// - `geoda::Column` es un handle con RAII (solo se mueve, nunca se copia): el destructor llama a
//   `geoda_column_free` una sola vez.
// - Todo error de la capi se convierte en una `geoda::Error` con el codigo, la categoria
//   (`INVALID_ARGUMENT`, `TYPE_MISMATCH`...) y el mensaje de `geoda_last_error_message`.
// - Una **matriz matematica** es `std::vector<std::vector<double>>` por **filas**; una tabla de datos
//   (`kmeans`) es un vector de **columnas**, una por variable (decision 014, N9).
// - Los hilos no se exponen: `workers` es siempre 1.
//
// Se enlaza con `-lgeoda`; ver `bindings/cpp/CMakeLists.txt`.
#ifndef GEODA_HPP
#define GEODA_HPP

#include <cstddef>
#include <cstdint>
#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <variant>
#include <vector>

#include "geoda.h"

namespace geoda {

// Nombre de la categoria de un codigo de error de la capi.
inline std::string_view category_of(int code) noexcept {
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

// Error de la capi.
class Error : public std::runtime_error {
 public:
  Error(int code, const std::string& message)
      : std::runtime_error(std::string(category_of(code)) + ": " + message), code_(code) {}
  // Codigo negativo `GEODA_ERR_*`.
  int code() const noexcept { return code_; }
  // Categoria como texto (`"INVALID_ARGUMENT"`...).
  std::string_view category() const noexcept { return category_of(code_); }

 private:
  int code_;
};

namespace detail {
inline void check(int code) {
  if (code != GEODA_OK) throw Error(code, geoda_last_error_message());
}
}  // namespace detail

// Version de la ABI contra la que se compilo la cabecera, como `major << 16 | minor`.
inline std::uint32_t abi_version() noexcept { return geoda_abi_version(); }

// Columna de geoda (handle con RAII).
class Column {
 public:
  Column() noexcept = default;
  // Toma posesion de un handle de la capi (puede ser NULL).
  explicit Column(GeodaColumn* handle) noexcept : handle_(handle) {}
  Column(const Column&) = delete;
  Column& operator=(const Column&) = delete;
  Column(Column&& other) noexcept : handle_(std::exchange(other.handle_, nullptr)) {}
  Column& operator=(Column&& other) noexcept {
    if (this != &other) {
      reset();
      handle_ = std::exchange(other.handle_, nullptr);
    }
    return *this;
  }
  ~Column() { reset(); }

  // El handle crudo (sigue siendo de la `Column`).
  GeodaColumn* get() const noexcept { return handle_; }
  // Suelta el handle: quien llama debe liberarlo con `geoda_column_free`.
  GeodaColumn* release() noexcept { return std::exchange(handle_, nullptr); }
  bool empty_handle() const noexcept { return handle_ == nullptr; }

  // --- Crear. `valid` vacio = todos validos; si no, un byte por fila (1 = valido, 0 = nulo).
  static Column from(const std::vector<double>& values, const std::vector<std::uint8_t>& valid = {}) {
    return make(values.size(), valid, [&](const std::uint8_t* v, GeodaColumn** out) {
      return geoda_column_from_f64(values.data(), values.size(), v, out);
    });
  }
  static Column from(const std::vector<std::int32_t>& values, const std::vector<std::uint8_t>& valid = {}) {
    return make(values.size(), valid, [&](const std::uint8_t* v, GeodaColumn** out) {
      return geoda_column_from_i32(values.data(), values.size(), v, out);
    });
  }
  static Column from(const std::vector<std::int64_t>& values, const std::vector<std::uint8_t>& valid = {}) {
    return make(values.size(), valid, [&](const std::uint8_t* v, GeodaColumn** out) {
      return geoda_column_from_i64(values.data(), values.size(), v, out);
    });
  }
  // Booleanos: un byte por valor (0 = falso).
  static Column from_bool(const std::vector<std::uint8_t>& values, const std::vector<std::uint8_t>& valid = {}) {
    return make(values.size(), valid, [&](const std::uint8_t* v, GeodaColumn** out) {
      return geoda_column_from_bool(values.data(), values.size(), v, out);
    });
  }
  // Texto UTF-8; `std::nullopt` es un nulo.
  static Column from_strings(const std::vector<std::optional<std::string>>& values) {
    std::string data;
    std::vector<std::int32_t> offsets{0};
    std::vector<std::uint8_t> valid;
    for (const auto& value : values) {
      valid.push_back(value.has_value() ? 1 : 0);
      if (value) data += *value;
      offsets.push_back(static_cast<std::int32_t>(data.size()));
    }
    GeodaColumn* out = nullptr;
    detail::check(geoda_column_from_utf8(reinterpret_cast<const std::uint8_t*>(data.data()), data.size(),
                                         offsets.data(), values.size(), valid.data(), &out));
    return Column(out);
  }

  // --- Leer.
  std::size_t size() const {
    const std::int64_t n = geoda_column_len(handle_);
    if (n < 0) detail::check(static_cast<int>(n));
    return static_cast<std::size_t>(n);
  }
  std::size_t null_count() const {
    const std::int64_t n = geoda_column_null_count(handle_);
    if (n < 0) detail::check(static_cast<int>(n));
    return static_cast<std::size_t>(n);
  }
  // Una de las constantes `GEODA_DTYPE_*`.
  int dtype() const {
    const int d = geoda_column_dtype(handle_);
    if (d < 0) detail::check(d);
    return d;
  }
  // Un byte por fila: 1 = valida, 0 = nula.
  std::vector<std::uint8_t> validity() const {
    std::vector<std::uint8_t> out(size());
    detail::check(geoda_column_copy_validity(handle_, out.data(), out.size()));
    return out;
  }
  // Los valores (un nulo vale 0). `T` es `double`, `std::int32_t` o `std::int64_t`.
  template <class T>
  std::vector<T> values() const;
  // Los valores con `std::nullopt` en los nulos.
  template <class T>
  std::vector<std::optional<T>> optional_values() const {
    const auto v = values<T>();
    const auto valid = validity();
    std::vector<std::optional<T>> out;
    out.reserve(v.size());
    for (std::size_t i = 0; i < v.size(); ++i) out.push_back(valid[i] ? std::optional<T>(v[i]) : std::nullopt);
    return out;
  }
  // Booleanos como bytes (0 o 1).
  std::vector<std::uint8_t> bools() const {
    std::vector<std::uint8_t> out(size());
    detail::check(geoda_column_copy_bool(handle_, out.data(), out.size()));
    return out;
  }
  // Texto: `std::nullopt` en los nulos.
  std::vector<std::optional<std::string>> strings() const {
    std::size_t total = 0;
    detail::check(geoda_column_utf8_data_len(handle_, &total));
    const std::size_t n = size();
    std::vector<std::uint8_t> data(total);
    std::vector<std::int32_t> offsets(n + 1);
    detail::check(geoda_column_copy_utf8(handle_, data.data(), total, offsets.data(), offsets.size()));
    const auto valid = validity();
    std::vector<std::optional<std::string>> out;
    out.reserve(n);
    for (std::size_t i = 0; i < n; ++i) {
      if (!valid[i]) {
        out.emplace_back(std::nullopt);
      } else {
        out.emplace_back(std::string(reinterpret_cast<const char*>(data.data()) + offsets[i],
                                     static_cast<std::size_t>(offsets[i + 1] - offsets[i])));
      }
    }
    return out;
  }

 private:
  void reset() noexcept {
    if (handle_ != nullptr) {
      geoda_column_free(handle_);
      handle_ = nullptr;
    }
  }

  template <class F>
  static Column make(std::size_t len, const std::vector<std::uint8_t>& valid, F&& call) {
    if (!valid.empty() && valid.size() != len) {
      throw Error(GEODA_ERR_LENGTH_MISMATCH, "valid mide " + std::to_string(valid.size()) + " y values " + std::to_string(len));
    }
    GeodaColumn* out = nullptr;
    detail::check(call(valid.empty() ? nullptr : valid.data(), &out));
    return Column(out);
  }

  GeodaColumn* handle_ = nullptr;
};

template <>
inline std::vector<double> Column::values<double>() const {
  std::vector<double> out(size());
  detail::check(geoda_column_copy_f64(handle_, out.data(), out.size()));
  return out;
}
template <>
inline std::vector<std::int32_t> Column::values<std::int32_t>() const {
  std::vector<std::int32_t> out(size());
  detail::check(geoda_column_copy_i32(handle_, out.data(), out.size()));
  return out;
}
template <>
inline std::vector<std::int64_t> Column::values<std::int64_t>() const {
  std::vector<std::int64_t> out(size());
  detail::check(geoda_column_copy_i64(handle_, out.data(), out.size()));
  return out;
}

// --- Estadisticos escalares (columnas `Int32`, `Int64` o `Float64`; ignoran los nulos).

using Number = std::variant<std::int64_t, double>;

namespace detail {
inline bool is_integer(const Column& column) {
  const int d = column.dtype();
  return d == GEODA_DTYPE_INT32 || d == GEODA_DTYPE_INT64;
}
}  // namespace detail

inline std::int64_t count(const Column& column) {
  std::int64_t out = 0;
  detail::check(geoda_column_count(column.get(), &out));
  return out;
}

// Suma, minimo y maximo conservan el tipo: `std::int64_t` para columnas enteras, `double` si no.
inline Number sum(const Column& column) {
  if (detail::is_integer(column)) {
    std::int64_t out = 0;
    detail::check(geoda_column_sum_i64(column.get(), 1, &out));
    return out;
  }
  double out = 0;
  detail::check(geoda_column_sum_f64(column.get(), 1, &out));
  return out;
}
inline Number min(const Column& column) {
  if (detail::is_integer(column)) {
    std::int64_t out = 0;
    detail::check(geoda_column_min_i64(column.get(), 1, &out));
    return out;
  }
  double out = 0;
  detail::check(geoda_column_min_f64(column.get(), 1, &out));
  return out;
}
inline Number max(const Column& column) {
  if (detail::is_integer(column)) {
    std::int64_t out = 0;
    detail::check(geoda_column_max_i64(column.get(), 1, &out));
    return out;
  }
  double out = 0;
  detail::check(geoda_column_max_f64(column.get(), 1, &out));
  return out;
}

inline double mean(const Column& column) {
  double out = 0;
  detail::check(geoda_column_mean(column.get(), 1, &out));
  return out;
}
// Varianza muestral por defecto (`ddof = 1`, como R y pandas); NumPy usa 0.
inline double var(const Column& column, std::size_t ddof = 1) {
  double out = 0;
  detail::check(geoda_column_var(column.get(), ddof, 1, &out));
  return out;
}
inline double std_dev(const Column& column, std::size_t ddof = 1) {
  double out = 0;
  detail::check(geoda_column_std(column.get(), ddof, 1, &out));
  return out;
}
inline double median(const Column& column) {
  double out = 0;
  detail::check(geoda_column_median(column.get(), &out));
  return out;
}
// `p` en `[0, 100]`, interpolacion lineal (tipo 7).
inline double percentile(const Column& column, double p) {
  double out = 0;
  detail::check(geoda_column_percentile(column.get(), p, &out));
  return out;
}
inline double corr(const Column& x, const Column& y) {
  double out = 0;
  detail::check(geoda_column_corr(x.get(), y.get(), 1, &out));
  return out;
}

// --- Texto.

// Distancia de edicion fila a fila entre dos columnas `Utf8` (las dos miden lo mismo, o una mide 1).
inline std::vector<std::optional<std::int64_t>> edit_distance(const Column& a, const Column& b) {
  GeodaColumn* out = nullptr;
  detail::check(geoda_column_edit_distance(a.get(), b.get(), &out));
  return Column(out).optional_values<std::int64_t>();
}
// Distancia de edicion (por caracteres Unicode) entre dos cadenas.
inline std::int64_t edit_distance(const std::string& a, const std::string& b) {
  const auto result = edit_distance(Column::from_strings({a}), Column::from_strings({b}));
  return result.at(0).value();
}

// --- Algebra lineal. Una matriz es una lista de filas.

using Matrix = std::vector<std::vector<double>>;

namespace detail {
// Las columnas de una matriz por filas, como handles para la capi.
struct ColumnList {
  std::vector<Column> columns;
  std::vector<const GeodaColumn*> pointers;
};

inline ColumnList columns_of(const Matrix& rows) {
  if (rows.empty()) throw Error(GEODA_ERR_INVALID_ARGUMENT, "la matriz no tiene filas");
  const std::size_t width = rows.front().size();
  for (const auto& row : rows) {
    if (row.size() != width) throw Error(GEODA_ERR_LENGTH_MISMATCH, "las filas miden distinto");
  }
  if (width == 0) throw Error(GEODA_ERR_INVALID_ARGUMENT, "la matriz no tiene columnas");
  ColumnList list;
  for (std::size_t j = 0; j < width; ++j) {
    std::vector<double> column;
    column.reserve(rows.size());
    for (const auto& row : rows) column.push_back(row[j]);
    list.columns.push_back(Column::from(column));
  }
  for (const auto& column : list.columns) list.pointers.push_back(column.get());
  return list;
}

inline Matrix reshape(const std::vector<double>& flat, std::size_t rows, std::size_t cols) {
  if (rows * cols != flat.size()) throw Error(GEODA_ERR_INTERNAL, "la forma no coincide con los datos");
  Matrix out(rows, std::vector<double>(cols));
  for (std::size_t i = 0; i < rows; ++i) {
    for (std::size_t j = 0; j < cols; ++j) out[i][j] = flat[i * cols + j];
  }
  return out;
}
}  // namespace detail

inline Matrix matmul(const Matrix& a, const Matrix& b) {
  const auto left = detail::columns_of(a);
  const auto right = detail::columns_of(b);
  GeodaColumn* out = nullptr;
  detail::check(geoda_matmul(left.pointers.data(), left.pointers.size(), right.pointers.data(),
                             right.pointers.size(), &out));
  return detail::reshape(Column(out).values<double>(), a.size(), b.front().size());
}
// Resuelve `a x = b` (LU con pivoteo parcial). Una matriz singular lanza `INVALID_ARGUMENT`.
inline std::vector<double> solve(const Matrix& a, const std::vector<double>& b) {
  const auto list = detail::columns_of(a);
  const Column rhs = Column::from(b);
  GeodaColumn* out = nullptr;
  detail::check(geoda_lu_solve(list.pointers.data(), list.pointers.size(), rhs.get(), &out));
  return Column(out).values<double>();
}
// Determinante; una matriz singular da 0 (no es un error).
inline double det(const Matrix& a) {
  const auto list = detail::columns_of(a);
  double out = 0;
  detail::check(geoda_det(list.pointers.data(), list.pointers.size(), &out));
  return out;
}
inline Matrix inverse(const Matrix& a) {
  const auto list = detail::columns_of(a);
  GeodaColumn* out = nullptr;
  detail::check(geoda_inverse(list.pointers.data(), list.pointers.size(), &out));
  return detail::reshape(Column(out).values<double>(), a.size(), a.size());
}

// --- K-means.

struct KMeansResult {
  std::vector<std::int64_t> labels;  // el grupo de cada fila
  Matrix centroids;                  // k filas de una valor por variable
  double inertia = 0;
  std::int64_t iterations = 0;
};

// `variables` es una lista de columnas (una por variable), sin nulos ni NaN. La semilla fija el
// resultado.
inline KMeansResult kmeans(const std::vector<std::vector<double>>& variables, std::size_t k,
                           std::uint64_t seed = 0, std::size_t max_iter = 100, double tol = 1e-4) {
  if (variables.empty()) throw Error(GEODA_ERR_INVALID_ARGUMENT, "no hay variables");
  std::vector<Column> columns;
  std::vector<const GeodaColumn*> pointers;
  for (const auto& variable : variables) columns.push_back(Column::from(variable));
  for (const auto& column : columns) pointers.push_back(column.get());
  GeodaColumn* labels = nullptr;
  GeodaColumn* centroids = nullptr;
  KMeansResult result;
  detail::check(geoda_kmeans(pointers.data(), pointers.size(), k, max_iter, tol, seed, &labels, &centroids,
                             &result.inertia, &result.iterations));
  result.labels = Column(labels).values<std::int64_t>();
  result.centroids = detail::reshape(Column(centroids).values<double>(), k, variables.size());
  return result;
}

}  // namespace geoda

#endif  // GEODA_HPP
