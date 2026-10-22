//! Tabla hash propia y conteo de valores distintos de una columna.
//!
//! # Que es una tabla hash
//! Buscar un valor en una lista sin orden obliga a mirarla toda (O(n)). Una tabla hash lo
//! evita repartiendo los valores en **cubos**: una *funcion hash* convierte cada clave en
//! un numero y ese numero decide en que cubo vive. Para buscar solo se mira un cubo, que
//! tiene pocos elementos, asi que insertar y buscar cuestan O(1) en promedio.
//!
//! Dos claves distintas pueden caer en el mismo cubo (una *colision*). Aqui cada cubo es
//! una lista corta (*encadenamiento*): se recorre comparando las claves de verdad. Por eso
//! una mala funcion hash solo vuelve lenta a la tabla, nunca incorrecta. Cuando la tabla se
//! llena (mas de 3/4 de elementos por cubo, en promedio) duplica sus cubos y recoloca todo,
//! para que las listas sigan cortas.
//!
//! # La funcion hash
//! FNV-1a de 64 bits: parte de un numero fijo y, por cada byte de la clave, hace un XOR con
//! el byte y multiplica por un primo. Es corta y facil de explicar. Como el cubo se elige
//! con los **bits bajos** del hash y FNV los mezcla poco, al final se pasa por el mezclado
//! de splitmix64, que hace que cada bit del resultado dependa de todos los bytes.

use geoda_core::{Column, DataType, Error};

/// FNV-1a de 64 bits sobre `bytes`.
#[must_use]
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Mezclado final de splitmix64: reparte el efecto de cada bit sobre todo el resultado.
#[must_use]
pub fn mix64(mut z: u64) -> u64 {
    z ^= z >> 30;
    z = z.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z ^= z >> 27;
    z = z.wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Hash de unos bytes: FNV-1a y mezclado final.
#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> u64 {
    mix64(fnv1a_64(bytes))
}

/// Una clave de [`HashTable`]: sabe calcular su hash y compararse con otra.
///
/// Dos claves que se consideran iguales (`same_key`) deben tener el mismo `hash_key`.
pub trait Key {
    /// Hash de la clave.
    fn hash_key(&self) -> u64;
    /// `true` si las dos claves son la misma.
    fn same_key(&self, other: &Self) -> bool;
}

impl Key for i32 {
    fn hash_key(&self) -> u64 {
        hash_bytes(&self.to_le_bytes())
    }
    fn same_key(&self, other: &Self) -> bool {
        self == other
    }
}

impl Key for i64 {
    fn hash_key(&self) -> u64 {
        hash_bytes(&self.to_le_bytes())
    }
    fn same_key(&self, other: &Self) -> bool {
        self == other
    }
}

impl Key for bool {
    fn hash_key(&self) -> u64 {
        hash_bytes(&[u8::from(*self)])
    }
    fn same_key(&self, other: &Self) -> bool {
        self == other
    }
}

impl Key for &str {
    fn hash_key(&self) -> u64 {
        hash_bytes(self.as_bytes())
    }
    fn same_key(&self, other: &Self) -> bool {
        self == other
    }
}

/// Clave `Float64`: se compara por **bits**, con todos los `NaN` normalizados a uno. Asi
/// todos los `NaN` son la misma clave y `-0.0` y `0.0` son claves distintas (coherente
/// con el orden total de la decision F2).
#[derive(Debug, Clone, Copy)]
pub struct F64Key(u64);

impl F64Key {
    /// Clave de `value`.
    #[must_use]
    pub fn new(value: f64) -> Self {
        Self(if value.is_nan() {
            f64::NAN.to_bits()
        } else {
            value.to_bits()
        })
    }

    /// El flotante de la clave (un `NaN` sale como el `NaN` canonico).
    #[must_use]
    pub fn value(self) -> f64 {
        f64::from_bits(self.0)
    }
}

impl Key for F64Key {
    fn hash_key(&self) -> u64 {
        hash_bytes(&self.0.to_le_bytes())
    }
    fn same_key(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// Cantidad inicial de cubos; siempre es una potencia de 2.
const INITIAL_BUCKETS: usize = 8;

/// Tabla hash con encadenamiento. Las claves son unicas; insertar una existente
/// reemplaza su valor.
pub struct HashTable<K: Key, V> {
    buckets: Vec<Vec<(K, V)>>,
    len: usize,
}

impl<K: Key, V> Default for HashTable<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Key, V> HashTable<K, V> {
    /// Tabla vacia.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buckets: Self::empty_buckets(INITIAL_BUCKETS),
            len: 0,
        }
    }

    fn empty_buckets(count: usize) -> Vec<Vec<(K, V)>> {
        (0..count).map(|_| Vec::new()).collect()
    }

    /// Cantidad de claves.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` si no hay claves.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Cubo de `hash`: como la cantidad es potencia de 2, la mascara de bits equivale a
    /// un modulo.
    #[allow(clippy::cast_possible_truncation)] // se enmascara a un indice < buckets.len()
    fn bucket_of(&self, hash: u64) -> usize {
        (hash as usize) & (self.buckets.len() - 1)
    }

    /// El valor de `key`, si esta.
    #[must_use]
    pub fn get(&self, key: &K) -> Option<&V> {
        let bucket = &self.buckets[self.bucket_of(key.hash_key())];
        bucket.iter().find(|(k, _)| k.same_key(key)).map(|(_, v)| v)
    }

    /// El valor de `key` para modificarlo, si esta.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let index = self.bucket_of(key.hash_key());
        self.buckets[index]
            .iter_mut()
            .find(|(k, _)| k.same_key(key))
            .map(|(_, v)| v)
    }

    /// Inserta `key` con `value`. Si la clave ya estaba, devuelve el valor anterior.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let index = self.bucket_of(key.hash_key());
        if let Some((_, old)) = self.buckets[index]
            .iter_mut()
            .find(|(k, _)| k.same_key(&key))
        {
            return Some(std::mem::replace(old, value));
        }
        self.buckets[index].push((key, value));
        self.len += 1;
        // Carga maxima: 3/4 de elemento por cubo.
        if self.len * 4 > self.buckets.len() * 3 {
            self.grow();
        }
        None
    }

    /// Duplica los cubos y recoloca todos los elementos.
    fn grow(&mut self) {
        let doubled = self.buckets.len() * 2;
        let old = std::mem::replace(&mut self.buckets, Self::empty_buckets(doubled));
        for (key, value) in old.into_iter().flatten() {
            let index = self.bucket_of(key.hash_key());
            self.buckets[index].push((key, value));
        }
    }
}

/// Un valor suelto de una columna, de cualquier tipo soportado.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Valor `Bool`.
    Bool(bool),
    /// Valor `Int32`.
    Int32(i32),
    /// Valor `Int64`.
    Int64(i64),
    /// Valor `Float64` (un `NaN` sale como el `NaN` canonico).
    Float64(f64),
    /// Valor `Utf8`.
    Utf8(String),
}

/// Valores distintos de una columna con su cantidad de apariciones.
#[derive(Debug, Clone, PartialEq)]
pub struct ValueCounts {
    /// Cada valor distinto y cuantas veces aparece, en el orden de su **primera
    /// aparicion** en la columna.
    pub counts: Vec<(Value, usize)>,
    /// Cantidad de nulos. No son una clave: se cuentan aparte (decision F1).
    pub nulls: usize,
}

/// Acumula conteos por clave: la tabla guarda en que posicion de `counts` vive cada clave.
struct Counter<K: Key> {
    table: HashTable<K, usize>,
    result: ValueCounts,
}

impl<K: Key> Counter<K> {
    fn new() -> Self {
        Self {
            table: HashTable::new(),
            result: ValueCounts {
                counts: Vec::new(),
                nulls: 0,
            },
        }
    }

    /// Cuenta una aparicion; `value_of` arma el [`Value`] solo la primera vez que sale la clave.
    fn add(&mut self, key: Option<K>, value_of: impl FnOnce(&K) -> Value) {
        let Some(key) = key else {
            self.result.nulls += 1;
            return;
        };
        if let Some(&position) = self.table.get(&key) {
            self.result.counts[position].1 += 1;
        } else {
            self.result.counts.push((value_of(&key), 1));
            self.table.insert(key, self.result.counts.len() - 1);
        }
    }
}

/// Valores distintos de una columna y cuantas veces aparece cada uno, en tiempo O(n)
/// esperado.
///
/// Los nulos no son una clave: se cuentan en [`ValueCounts::nulls`] (decision F1). En
/// `Float64`, todos los `NaN` son una sola clave y `-0.0` y `0.0` son distintas. La
/// cadena vacia en `Utf8` es una clave mas.
///
/// # Errors
/// Los de acceso a la columna (no deberian ocurrir con una columna valida).
pub fn value_counts_column(column: &Column) -> Result<ValueCounts, Error> {
    let len = column.len();
    match column.dtype() {
        DataType::Bool => {
            let mut counter = Counter::new();
            for i in 0..len {
                let key = if column.is_null(i)? {
                    None
                } else {
                    Some(column.bool_value(i)?)
                };
                counter.add(key, |&k| Value::Bool(k));
            }
            Ok(counter.result)
        }
        DataType::Int32 => primitive_counts(column, Value::Int32),
        DataType::Int64 => primitive_counts(column, Value::Int64),
        DataType::Float64 => {
            let mut counter = Counter::new();
            let values = column.values::<f64>()?;
            for (i, &v) in values.iter().enumerate() {
                let key = if column.is_null(i)? {
                    None
                } else {
                    Some(F64Key::new(v))
                };
                counter.add(key, |k| Value::Float64(k.value()));
            }
            Ok(counter.result)
        }
        DataType::Utf8 => {
            let mut counter = Counter::new();
            for i in 0..len {
                let key = if column.is_null(i)? {
                    None
                } else {
                    Some(column.str_value(i)?)
                };
                counter.add(key, |&k| Value::Utf8(k.to_owned()));
            }
            Ok(counter.result)
        }
    }
}

/// Conteo de una columna `Int32` o `Int64`.
fn primitive_counts<T: Key + geoda_core::memory::Native>(
    column: &Column,
    wrap: fn(T) -> Value,
) -> Result<ValueCounts, Error> {
    let mut counter = Counter::new();
    for (i, &v) in column.values::<T>()?.iter().enumerate() {
        let key = if column.is_null(i)? { None } else { Some(v) };
        counter.add(key, |&k| wrap(k));
    }
    Ok(counter.result)
}

#[cfg(test)]
mod tests {
    use geoda_core::{BoolBuilder, PrimitiveBuilder, Utf8Builder};

    use super::*;

    /// Generador pseudoaleatorio con semilla fija (xorshift64), igual que en `select`.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, bound: u64) -> u64 {
            self.next() % bound
        }
    }

    /// Clave cuyo hash es siempre el mismo: fuerza todas las colisiones posibles.
    #[derive(Debug, Clone, Copy)]
    struct Colliding(u32);

    impl Key for Colliding {
        fn hash_key(&self) -> u64 {
            7
        }
        fn same_key(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }

    fn int64_column(values: &[Option<i64>]) -> Column {
        let mut builder = PrimitiveBuilder::<i64>::new();
        for value in values {
            match value {
                Some(v) => builder.append(*v),
                None => builder.append_null(),
            }
        }
        builder.finish().unwrap()
    }

    fn utf8_column(values: &[Option<&str>]) -> Column {
        let mut builder = Utf8Builder::new();
        for value in values {
            match value {
                Some(s) => builder.append(s).unwrap(),
                None => builder.append_null(),
            }
        }
        builder.finish().unwrap()
    }

    #[test]
    fn fnv1a_matches_the_published_test_vectors() {
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn the_mix_spreads_consecutive_integers_over_the_low_bits() {
        // Sin mezclar, enteros consecutivos caerian en cubos casi consecutivos; con el
        // mezclado deben repartirse por todos los cubos de una tabla chica.
        let mut seen = [false; 16];
        for n in 0_i64..64 {
            seen[usize::try_from(n.hash_key() & 15).unwrap()] = true;
        }
        assert!(seen.iter().all(|&s| s), "cubos sin usar: {seen:?}");
    }

    #[test]
    fn insert_get_replace_and_grow() {
        let mut table = HashTable::new();
        assert!(table.is_empty());
        assert_eq!(table.get(&1_i64), None);
        for n in 0_i64..1000 {
            assert_eq!(table.insert(n, n * 10), None);
        }
        assert_eq!(table.len(), 1000);
        // Tras crecer varias veces nada se perdio ni cambio.
        for n in 0_i64..1000 {
            assert_eq!(table.get(&n), Some(&(n * 10)), "clave {n}");
        }
        assert_eq!(table.get(&1000), None);
        assert_eq!(table.insert(5, -1), Some(50));
        assert_eq!(table.len(), 1000);
        *table.get_mut(&5).unwrap() += 1;
        assert_eq!(table.get(&5), Some(&0));
        assert!(table.get_mut(&1000).is_none());
        // La carga se mantiene: el hash reparte, no hay un cubo con cientos de claves.
        let longest = table.buckets.iter().map(Vec::len).max().unwrap();
        assert!(longest < 20, "cubo mas largo: {longest}");
    }

    #[test]
    fn forced_collisions_stay_correct() {
        let mut table = HashTable::new();
        for n in 0..500 {
            assert_eq!(table.insert(Colliding(n), n), None);
        }
        assert_eq!(table.len(), 500);
        for n in 0..500 {
            assert_eq!(table.get(&Colliding(n)), Some(&n));
        }
        assert_eq!(table.get(&Colliding(500)), None);
        assert_eq!(table.insert(Colliding(250), 0), Some(250));
        assert_eq!(table.len(), 500);
    }

    #[test]
    fn counts_of_a_small_int64_column() {
        let column = int64_column(&[Some(1), Some(2), Some(2), Some(3), Some(3), Some(3)]);
        let counts = value_counts_column(&column).unwrap();
        assert_eq!(counts.nulls, 0);
        assert_eq!(
            counts.counts,
            [
                (Value::Int64(1), 1),
                (Value::Int64(2), 2),
                (Value::Int64(3), 3)
            ]
        );
    }

    #[test]
    fn utf8_empty_string_is_a_key_and_nulls_are_counted_apart() {
        let column = utf8_column(&[
            Some(""),
            Some("a"),
            None,
            Some(""),
            Some("a"),
            Some(""),
            None,
        ]);
        let counts = value_counts_column(&column).unwrap();
        assert_eq!(counts.nulls, 2);
        assert_eq!(
            counts.counts,
            [
                (Value::Utf8(String::new()), 3),
                (Value::Utf8("a".to_string()), 2)
            ]
        );
    }

    #[test]
    fn float64_nan_is_one_key_and_signed_zeros_are_distinct() {
        let mut builder = PrimitiveBuilder::<f64>::new();
        let other_nan = f64::from_bits(f64::NAN.to_bits() | 1); // otro patron de NaN
        for value in [f64::NAN, -0.0, 0.0, other_nan, -0.0, f64::NAN, 1.5] {
            builder.append(value);
        }
        builder.append_null();
        let counts = value_counts_column(&builder.finish().unwrap()).unwrap();
        assert_eq!(counts.nulls, 1);
        assert_eq!(counts.counts.len(), 4);
        let count_of = |want: f64| {
            counts
                .counts
                .iter()
                .find(|(v, _)| matches!(v, Value::Float64(x) if x.to_bits() == want.to_bits() || (x.is_nan() && want.is_nan())))
                .map(|&(_, n)| n)
        };
        assert_eq!(count_of(f64::NAN), Some(3));
        assert_eq!(count_of(-0.0), Some(2));
        assert_eq!(count_of(0.0), Some(1));
        assert_eq!(count_of(1.5), Some(1));
    }

    #[test]
    fn int32_bool_empty_and_sliced_columns() {
        let mut ints = PrimitiveBuilder::<i32>::new();
        for value in [7, 7, -1] {
            ints.append(value);
        }
        assert_eq!(
            value_counts_column(&ints.finish().unwrap()).unwrap().counts,
            [(Value::Int32(7), 2), (Value::Int32(-1), 1)]
        );

        let mut bools = BoolBuilder::new();
        bools.append(true);
        bools.append(false);
        bools.append(true);
        bools.append_null();
        let counts = value_counts_column(&bools.finish().unwrap()).unwrap();
        assert_eq!(
            counts.counts,
            [(Value::Bool(true), 2), (Value::Bool(false), 1)]
        );
        assert_eq!(counts.nulls, 1);

        let empty = value_counts_column(&int64_column(&[])).unwrap();
        assert!(empty.counts.is_empty() && empty.nulls == 0);

        let column = int64_column(&[Some(9), Some(4), None, Some(4), Some(1)]);
        let window = column.slice(1, 3).unwrap(); // [4, NULL, 4]
        let counts = value_counts_column(&window).unwrap();
        assert_eq!(counts.counts, [(Value::Int64(4), 2)]);
        assert_eq!(counts.nulls, 1);
    }

    #[test]
    fn matches_the_naive_count_on_1000_seeded_inputs() {
        let mut rng = Rng(0x4A5_0013);
        for case in 0..1000 {
            let len = usize::try_from(rng.below(60)).unwrap();
            // Rango chico para muchos repetidos; el segundo para casi todos distintos.
            let range = [5, 1_000_000][case % 2];
            let data: Vec<Option<i64>> = (0..len)
                .map(|_| (rng.below(8) != 0).then(|| i64::try_from(rng.below(range)).unwrap() - 2))
                .collect();
            // Oraculo O(n^2): para cada valor se busca linealmente su entrada.
            let mut naive: Vec<(i64, usize)> = Vec::new();
            let mut naive_nulls = 0;
            for value in &data {
                match value {
                    None => naive_nulls += 1,
                    Some(v) => match naive.iter_mut().find(|(k, _)| k == v) {
                        Some(entry) => entry.1 += 1,
                        None => naive.push((*v, 1)),
                    },
                }
            }
            let got = value_counts_column(&int64_column(&data)).unwrap();
            let expected: Vec<(Value, usize)> = naive
                .into_iter()
                .map(|(k, n)| (Value::Int64(k), n))
                .collect();
            assert_eq!(got.counts, expected, "caso {case}: {data:?}");
            assert_eq!(got.nulls, naive_nulls, "caso {case}");
        }
    }
}
