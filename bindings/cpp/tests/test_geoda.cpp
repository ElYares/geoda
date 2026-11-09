// Pruebas del envoltorio C++ (HU-062). Los valores esperados salen de `spec/cases/`: el binding no
// tiene logica propia, solo comprueba que memoria, tipos y errores atraviesan la frontera bien.
#include <cmath>
#include <iostream>
#include <string>

#include <geoda/geoda.hpp>

namespace {
int failures = 0;

#define CHECK(cond)                                                              \
  do {                                                                           \
    if (!(cond)) {                                                               \
      std::cerr << __FILE__ << ":" << __LINE__ << ": fallo: " #cond << std::endl; \
      ++failures;                                                                \
    }                                                                            \
  } while (0)

bool near(double a, double b, double tol) { return std::fabs(a - b) <= tol; }

// Ejecuta `body` y comprueba que lanza `geoda::Error` con la categoria dada.
template <class F>
bool throws(const char* category, F&& body) {
  try {
    body();
  } catch (const geoda::Error& e) {
    return e.category() == category && e.code() < 0 && std::string(e.what()).find(category) == 0;
  }
  return false;
}

void columns_round_trip() {
  const auto f = geoda::Column::from(std::vector<double>{1.5, 99.0, 3.5}, {1, 0, 1});
  CHECK(f.size() == 3 && f.null_count() == 1 && f.dtype() == GEODA_DTYPE_FLOAT64);
  const auto optional = f.optional_values<double>();
  CHECK(optional[0] == 1.5 && !optional[1].has_value() && optional[2] == 3.5);
  CHECK((f.validity() == std::vector<std::uint8_t>{1, 0, 1}));

  const auto i32 = geoda::Column::from(std::vector<std::int32_t>{-2147483647 - 1, 0, 2147483647});
  CHECK((i32.values<std::int32_t>() == std::vector<std::int32_t>{-2147483647 - 1, 0, 2147483647}));
  const auto i64 = geoda::Column::from(std::vector<std::int64_t>{INT64_MIN, 0, INT64_MAX});
  CHECK(i64.values<std::int64_t>()[0] == INT64_MIN && i64.values<std::int64_t>()[2] == INT64_MAX);
  const auto flags = geoda::Column::from_bool({1, 0, 1}, {1, 1, 0});
  CHECK((flags.bools() == std::vector<std::uint8_t>{1, 0, 0}) && flags.null_count() == 1);

  const auto text = geoda::Column::from_strings({"", std::string("\xC3\xB1" "and\xC3\xBA"), std::nullopt, "hola"});
  const auto strings = text.strings();
  CHECK(strings.size() == 4 && strings[0] == "" && strings[1] == "\xC3\xB1" "and\xC3\xBA" && !strings[2] && strings[3] == "hola");

  const auto empty = geoda::Column::from(std::vector<double>{});
  CHECK(empty.size() == 0 && empty.values<double>().empty());
}

void ownership() {
  auto a = geoda::Column::from(std::vector<double>{1.0, 2.0});
  geoda::Column b = std::move(a);
  CHECK(a.empty_handle() && b.size() == 2);
  geoda::Column c;
  c = std::move(b);  // b queda vacia; c libera lo que tenia (nada)
  CHECK(b.empty_handle() && c.size() == 2);
  GeodaColumn* raw = c.release();
  CHECK(c.empty_handle() && raw != nullptr);
  geoda_column_free(raw);
}

void errors() {
  CHECK(throws("INVALID_ARGUMENT", [] { geoda::median(geoda::Column::from(std::vector<double>{})); }));
  CHECK(throws("TYPE_MISMATCH", [] { geoda::Column::from(std::vector<double>{1.0}).values<std::int64_t>(); }));
  CHECK(throws("LENGTH_MISMATCH", [] { geoda::Column::from(std::vector<double>{1.0, 2.0}, {1}); }));
  CHECK(throws("INVALID_ARGUMENT", [] { geoda::solve({{1, 2}, {2, 4}}, {1, 2}); }));
  CHECK(throws("LENGTH_MISMATCH", [] { geoda::matmul({{1, 2, 3}, {4, 5, 6}}, {{1, 2, 3}, {4, 5, 6}}); }));
  CHECK(throws("LENGTH_MISMATCH", [] { geoda::det({{1, 2}, {3}}); }));
  CHECK(throws("INVALID_ARGUMENT", [] { geoda::kmeans({{0, 1, 2}}, 0); }));
  try {
    geoda::median(geoda::Column::from(std::vector<double>{}));
  } catch (const geoda::Error& e) {
    CHECK(e.code() == GEODA_ERR_INVALID_ARGUMENT);
  }
}

void scalars() {
  const auto ints = geoda::Column::from(std::vector<std::int64_t>{1, 2, 3, 4});
  CHECK(geoda::count(ints) == 4);
  CHECK(std::get<std::int64_t>(geoda::sum(ints)) == 10);
  CHECK(std::get<std::int64_t>(geoda::min(ints)) == 1 && std::get<std::int64_t>(geoda::max(ints)) == 4);
  CHECK(geoda::median(ints) == 2.5);  // spec: column/median
  CHECK(geoda::mean(ints) == 2.5);
  CHECK(near(geoda::var(ints), 5.0 / 3.0, 1e-12) && near(geoda::var(ints, 0), 1.25, 1e-12));
  CHECK(near(geoda::std_dev(ints, 0), std::sqrt(1.25), 1e-12));
  CHECK(near(geoda::percentile(ints, 25.0), 1.75, 1e-12));
  const auto floats = geoda::Column::from(std::vector<double>{1.5, 0.0, 2.5}, {1, 0, 1});
  CHECK(std::get<double>(geoda::sum(floats)) == 4.0 && geoda::count(floats) == 2);
  const auto y = geoda::Column::from(std::vector<double>{2.0, 4.0, 6.0, 8.0});
  CHECK(near(geoda::corr(ints, y), 1.0, 1e-12));
}

void text() {
  CHECK(geoda::edit_distance("kitten", "sitting") == 3);
  CHECK(geoda::edit_distance("a\xC3\xB1" "b", "anb") == 1);  // por caracteres, no bytes
  const auto many = geoda::edit_distance(geoda::Column::from_strings({"casa", "cosa", std::nullopt}),
                                         geoda::Column::from_strings({"casa"}));
  CHECK(many[0] == 0 && many[1] == 1 && !many[2]);
}

void linear_algebra() {
  const auto product = geoda::matmul({{1, 2, 3}, {4, 5, 6}}, {{7, 8}, {9, 10}, {11, 12}});
  CHECK(product.size() == 2 && product[0] == (std::vector<double>{58, 64}) && product[1] == (std::vector<double>{139, 154}));
  const auto x = geoda::solve({{2, 1}, {1, 3}}, {3, 5});
  CHECK(near(x[0], 0.8, 1e-12) && near(x[1], 1.4, 1e-12));
  CHECK(geoda::solve({{0, 1}, {1, 0}}, {2, 3}) == (std::vector<double>{3, 2}));
  CHECK(near(geoda::det({{4, 3}, {6, 3}}), -6.0, 1e-12));
  CHECK(geoda::det({{1, 2}, {2, 4}}) == 0.0);
  const auto inv = geoda::inverse({{4, 7}, {2, 6}});
  CHECK(near(inv[0][0], 0.6, 1e-12) && near(inv[0][1], -0.7, 1e-12) && near(inv[1][0], -0.2, 1e-12) && near(inv[1][1], 0.4, 1e-12));
}

void clustering() {
  const auto result = geoda::kmeans({{0, 0, 10, 10}, {0, 1, 10, 11}}, 2, 1);  // spec: cluster/kmeans_two_clouds
  CHECK((result.labels == std::vector<std::int64_t>{0, 0, 1, 1}));
  CHECK(result.centroids[0] == (std::vector<double>{0.0, 0.5}) && result.centroids[1] == (std::vector<double>{10.0, 10.5}));
  CHECK(result.inertia == 1.0 && result.iterations >= 1);
  CHECK(geoda::kmeans({{0, 0, 10, 10}, {0, 1, 10, 11}}, 2, 1).labels == result.labels);  // determinista
}

void level_two() {
  const auto f = geoda::qr({{3, 0}, {4, 5}});  // spec: linalg/qr_3_4_5
  CHECK(near(f.q[0][0], -0.6, 1e-12) && near(f.q[0][1], 0.8, 1e-12) && near(f.q[1][0], -0.8, 1e-12) && near(f.q[1][1], -0.6, 1e-12));
  CHECK(near(f.r[0][0], -5, 1e-12) && near(f.r[0][1], -4, 1e-12) && f.r[1][0] == 0.0 && near(f.r[1][1], -3, 1e-12));
  const auto d = geoda::svd({{1, 0}, {0, 2}, {0, 0}});  // spec: linalg/svd_permutation_exact
  CHECK((d.s == std::vector<double>{2, 1}));
  CHECK((d.u == geoda::Matrix{{0, 1}, {1, 0}, {0, 0}}) && (d.vt == geoda::Matrix{{0, 1}, {1, 0}}));
  CHECK(geoda::svd({{1, 0, 0}, {0, 3, 0}}).s.size() == 2);  // matriz ancha
  const auto fit = geoda::lstsq({{1, 0}, {1, 1}, {1, 2}}, {1, 3, 5});
  CHECK(near(fit.coefficients[0], 1, 1e-12) && near(fit.coefficients[1], 2, 1e-12) && near(fit.residual_norm, 0, 1e-12));
  CHECK(throws("INVALID_ARGUMENT", [] { geoda::qr({{1, 2, 3}}); }));
  CHECK(throws("INVALID_ARGUMENT", [] { geoda::lstsq({{1, 2}, {2, 4}, {3, 6}}, {1, 2, 3}); }));

  const auto series = geoda::Column::from(std::vector<std::int64_t>{10, 9, 2, 5, 3, 7, 101, 18});
  CHECK((geoda::lis(series) == std::vector<std::int64_t>{2, 4, 5, 7}));
  CHECK(throws("INVALID_ARGUMENT", [] { geoda::lis(geoda::Column::from(std::vector<double>{1.0, std::nan(""), 3.0})); }));
  const auto names = geoda::Column::from_strings({"Juan Perez", std::string("Juan P\xC3\xA9rez"), "Ana", std::nullopt, "Ana"});
  const auto groups = geoda::dedupe(names, 1);
  CHECK(groups.count == 2 && groups.ids[0] == 0 && groups.ids[1] == 0 && groups.ids[2] == 1 && !groups.ids[3] && groups.ids[4] == 1);
  CHECK(throws("TYPE_MISMATCH", [&] { geoda::dedupe(series, 1); }));
  const auto values = geoda::Column::from(std::vector<std::int64_t>{1, 2, 3, 4, 5, 6});
  const auto h = geoda::histogram(values, 2, std::make_pair(2.0, 4.0));
  CHECK((h.edges == std::vector<double>{2, 3, 4}) && (h.counts == std::vector<std::int64_t>{1, 2}));
  CHECK(h.below == 1 && h.above == 2 && h.nulls == 0 && h.nans == 0);
  CHECK(throws("INVALID_ARGUMENT", [&] { geoda::histogram(values, 0); }));
}

void abi() { CHECK(geoda::abi_version() == (1u << 16)); }
}  // namespace

int main() {
  abi();
  columns_round_trip();
  ownership();
  errors();
  scalars();
  text();
  linear_algebra();
  clustering();
  level_two();
  if (failures == 0) {
    std::cout << "geoda_cpp_test: ok" << std::endl;
    return 0;
  }
  std::cerr << "geoda_cpp_test: " << failures << " fallos" << std::endl;
  return 1;
}
