//! Conjuntos de items frecuentes y reglas de asociacion (algoritmo Apriori).
//!
//! # El problema
//! Dadas muchas *transacciones* (cada una, un conjunto de items: una compra, una sesion), buscar
//! los conjuntos de items que aparecen juntos al menos `min_support` veces y, a partir de ellos,
//! reglas «quien lleva A tambien lleva C».
//!
//! # La idea de Apriori
//! Si un conjunto es frecuente, **todos sus subconjuntos tambien lo son** (aparecen al menos
//! tantas veces). Al reves: si un subconjunto no es frecuente, ningun conjunto que lo contenga
//! puede serlo. Entonces se construye por niveles:
//! 1. Nivel 1: los items sueltos con soporte `>= min_support`.
//! 2. Nivel `k`: se unen pares de conjuntos del nivel `k - 1` que comparten los primeros `k - 2`
//!    items (asi cada candidato sale una sola vez), se **podan** los candidatos con algun
//!    subconjunto de tamano `k - 1` que no sea frecuente, y se cuenta cuantas transacciones
//!    contienen cada candidato que sobrevivio.
//! 3. Se para cuando un nivel queda vacio.
//!
//! # Soporte, confianza y lift
//! El **soporte** es un conteo absoluto de transacciones (decision 013, M7), no una fraccion.
//! Para una regla `A -> C` con `S = A u C`:
//! - confianza = soporte(S) / soporte(A): de las transacciones con A, cuantas tienen tambien C;
//! - lift = confianza / (soporte(C) / N): cuanto mas probable es C dado A que sin saberlo. Mayor
//!   que 1, A y C van juntos mas de lo esperado por azar; menor que 1, menos.
//!
//! # Orden del resultado (M8)
//! Los conjuntos salen por tamano y, dentro de cada tamano, en orden lexicografico de los items.
//! Las reglas, por conjunto en ese orden y, dentro de cada uno, por la mascara de bits del
//! antecedente. Cada corrida da lo mismo.
//!
//! # Entrada
//! Transacciones e items son enteros `>= 0`; un item repetido dentro de una transaccion cuenta
//! una vez. En formato de columnas es el **formato largo**: dos columnas `Int64` del mismo largo,
//! `transaction` e `item`, una fila por par.
//!
//! # Limite de las reglas
//! De un conjunto de `m` items salen `2^m - 2` reglas posibles: con mas de [`MAX_RULE_ITEMS`]
//! items se devuelve `InvalidArgument` en vez de agotar la memoria.

use geoda_core::{Column, DataType, Error};

use crate::sort::merge_sort;

/// Tamano maximo de un conjunto frecuente del que se generan reglas.
pub const MAX_RULE_ITEMS: usize = 20;

/// Transacciones ya normalizadas: items densos, ordenados y sin repetidos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transactions {
    /// Id original de cada item denso, de menor a mayor.
    items: Vec<i64>,
    /// Cada transaccion: items densos ascendentes.
    rows: Vec<Vec<u32>>,
}

impl Transactions {
    /// Desde formato largo: `transaction[i]` contiene a `item[i]`.
    ///
    /// # Errors
    /// [`Error::TypeMismatch`] si no son `Int64`; [`Error::LengthMismatch`] si miden distinto;
    /// [`Error::InvalidArgument`] si hay nulos o ids negativos.
    pub fn from_columns(transaction: &Column, item: &Column) -> Result<Self, Error> {
        for (name, column) in [("transaction", transaction), ("item", item)] {
            if column.dtype() != DataType::Int64 {
                return Err(Error::TypeMismatch(format!(
                    "{name} debe ser Int64, no {:?}",
                    column.dtype()
                )));
            }
            if column.null_count() > 0 {
                return Err(Error::InvalidArgument(format!("{name} tiene nulos")));
            }
        }
        if transaction.len() != item.len() {
            return Err(Error::LengthMismatch(format!(
                "transaction mide {} e item {}",
                transaction.len(),
                item.len()
            )));
        }
        let pairs: Vec<(i64, i64)> = transaction
            .values::<i64>()?
            .iter()
            .copied()
            .zip(item.values::<i64>()?.iter().copied())
            .collect();
        Self::from_pairs(pairs)
    }

    /// Desde una lista de transacciones, cada una con sus items.
    ///
    /// # Errors
    /// [`Error::InvalidArgument`] si algun item es negativo.
    pub fn from_lists(lists: &[Vec<i64>]) -> Result<Self, Error> {
        let mut pairs = Vec::new();
        for (id, list) in lists.iter().enumerate() {
            let id = i64::try_from(id)
                .map_err(|_| Error::OutOfBounds("demasiadas transacciones".into()))?;
            pairs.extend(list.iter().map(|&item| (id, item)));
        }
        let mut transactions = Self::from_pairs(pairs)?;
        // Una transaccion vacia sigue contando en N: from_pairs solo ve las que tienen items.
        let empty = lists.len() - transactions.rows.len();
        transactions
            .rows
            .extend(std::iter::repeat_n(Vec::new(), empty));
        Ok(transactions)
    }

    fn from_pairs(mut pairs: Vec<(i64, i64)>) -> Result<Self, Error> {
        if let Some(&(tx, item)) = pairs.iter().find(|&&(tx, item)| tx < 0 || item < 0) {
            return Err(Error::InvalidArgument(format!(
                "ids negativos: transaccion {tx}, item {item}"
            )));
        }
        merge_sort(&mut pairs, Ord::cmp);
        pairs.dedup();
        let mut items: Vec<i64> = pairs.iter().map(|&(_, item)| item).collect();
        merge_sort(&mut items, Ord::cmp);
        items.dedup();
        let mut rows: Vec<Vec<u32>> = Vec::new();
        let mut current = None;
        for &(tx, item) in &pairs {
            let dense = u32::try_from(items.partition_point(|&i| i < item))
                .map_err(|_| Error::OutOfBounds("demasiados items distintos".into()))?;
            match rows.last_mut() {
                Some(row) if current == Some(tx) => row.push(dense),
                _ => {
                    rows.push(vec![dense]);
                    current = Some(tx);
                }
            }
        }
        Ok(Self { items, rows })
    }

    /// Cantidad de transacciones.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// `true` si no hay transacciones.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Conjuntos frecuentes, por tamano y luego lexicografico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrequentItemsets {
    /// Cada conjunto: sus items originales de menor a mayor.
    pub itemsets: Vec<Vec<i64>>,
    /// Transacciones que contienen cada conjunto.
    pub support: Vec<usize>,
    /// Transacciones totales (para el lift).
    pub transactions: usize,
}

/// Una regla `antecedent -> consequent`; los dos son indices en [`FrequentItemsets::itemsets`].
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    /// Conjunto del antecedente.
    pub antecedent: usize,
    /// Conjunto del consecuente.
    pub consequent: usize,
    /// Transacciones que contienen antecedente y consecuente.
    pub support: usize,
    /// `soporte(A u C) / soporte(A)`.
    pub confidence: f64,
    /// `confianza / (soporte(C) / N)`.
    pub lift: f64,
}

/// Conjuntos frecuentes con soporte `>= min_support`.
///
/// # Errors
/// [`Error::InvalidArgument`] si `min_support` es 0.
pub fn frequent_itemsets(
    transactions: &Transactions,
    min_support: usize,
) -> Result<FrequentItemsets, Error> {
    if min_support == 0 {
        return Err(Error::InvalidArgument("min_support debe ser >= 1".into()));
    }
    let mut result = FrequentItemsets {
        itemsets: Vec::new(),
        support: Vec::new(),
        transactions: transactions.len(),
    };
    // Nivel 1.
    let mut counts = vec![0usize; transactions.items.len()];
    for row in &transactions.rows {
        for &item in row {
            counts[item as usize] += 1;
        }
    }
    let mut level: Vec<Vec<u32>> = Vec::new();
    for (dense, &count) in (0u32..).zip(&counts) {
        if count >= min_support {
            level.push(vec![dense]);
            result.support.push(count);
        }
    }
    // Las transacciones sin items infrecuentes son mas cortas de recorrer.
    let frequent: Vec<bool> = counts.iter().map(|&c| c >= min_support).collect();
    let rows: Vec<Vec<u32>> = transactions
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .copied()
                .filter(|&i| frequent[i as usize])
                .collect()
        })
        .collect();
    push_level(&mut result, transactions, &level);

    let mut size = 1;
    while !level.is_empty() {
        size += 1;
        let candidates = candidates(&level);
        let mut next = Vec::new();
        let mut next_support = Vec::new();
        for candidate in candidates {
            let count = rows
                .iter()
                .filter(|row| row.len() >= size && contains_all(row, &candidate))
                .count();
            if count >= min_support {
                next.push(candidate);
                next_support.push(count);
            }
        }
        result.support.extend(&next_support);
        push_level(&mut result, transactions, &next);
        level = next;
    }
    Ok(result)
}

/// Agrega los conjuntos de un nivel (con sus items originales) al resultado.
fn push_level(result: &mut FrequentItemsets, transactions: &Transactions, level: &[Vec<u32>]) {
    for set in level {
        result.itemsets.push(
            set.iter()
                .map(|&i| transactions.items[i as usize])
                .collect(),
        );
    }
}

/// Candidatos del nivel siguiente: une pares con el mismo prefijo y poda los que tienen algun
/// subconjunto que no es frecuente. `level` esta en orden lexicografico.
fn candidates(level: &[Vec<u32>]) -> Vec<Vec<u32>> {
    let mut out = Vec::new();
    for (i, first) in level.iter().enumerate() {
        let prefix = &first[..first.len() - 1];
        for second in &level[i + 1..] {
            if &second[..second.len() - 1] != prefix {
                break; // ordenado: ya no comparten prefijo
            }
            let mut candidate = first.clone();
            candidate.push(second[second.len() - 1]);
            // Subconjuntos de tamano k-1: quitar cada item (los dos ultimos ya estan en `level`).
            let all_frequent = (0..candidate.len() - 2).all(|skip| {
                let subset: Vec<u32> = candidate
                    .iter()
                    .enumerate()
                    .filter(|&(pos, _)| pos != skip)
                    .map(|(_, &item)| item)
                    .collect();
                level.binary_search(&subset).is_ok()
            });
            if all_frequent {
                out.push(candidate);
            }
        }
    }
    out
}

/// `true` si la fila ordenada `row` contiene todos los items (ordenados) de `set`.
fn contains_all(row: &[u32], set: &[u32]) -> bool {
    let mut at = 0;
    for &item in set {
        while at < row.len() && row[at] < item {
            at += 1;
        }
        if at == row.len() || row[at] != item {
            return false;
        }
        at += 1;
    }
    true
}

/// Reglas con confianza `>= min_confidence` a partir de los conjuntos frecuentes.
///
/// # Errors
/// [`Error::InvalidArgument`] si `min_confidence` no esta en `(0, 1]` o un conjunto tiene mas de
/// [`MAX_RULE_ITEMS`] items, o `frequent` no contiene los subconjuntos de alguno de sus conjuntos.
pub fn association_rules(
    frequent: &FrequentItemsets,
    min_confidence: f64,
) -> Result<Vec<Rule>, Error> {
    if !(min_confidence > 0.0 && min_confidence <= 1.0) {
        return Err(Error::InvalidArgument(format!(
            "min_confidence debe estar en (0, 1], no {min_confidence}"
        )));
    }
    // Por Apriori todo subconjunto de un conjunto frecuente esta en la lista; si no, la entrada
    // no salio de `frequent_itemsets`.
    let index_of = |set: &[i64]| -> Result<usize, Error> {
        frequent
            .itemsets
            .binary_search_by(|probe| {
                probe
                    .len()
                    .cmp(&set.len())
                    .then_with(|| probe.as_slice().cmp(set))
            })
            .map_err(|_| Error::InvalidArgument(format!("falta el subconjunto {set:?}")))
    };
    let total = to_f64(frequent.transactions);
    let mut rules = Vec::new();
    for (id, set) in frequent.itemsets.iter().enumerate() {
        let size = set.len();
        if size < 2 {
            continue;
        }
        if size > MAX_RULE_ITEMS {
            return Err(Error::InvalidArgument(format!(
                "un conjunto de {size} items da demasiadas reglas (maximo {MAX_RULE_ITEMS})"
            )));
        }
        for mask in 1u32..(1 << size) - 1 {
            let (mut antecedent, mut consequent) = (Vec::new(), Vec::new());
            for (bit, &item) in set.iter().enumerate() {
                if mask >> bit & 1 == 1 {
                    antecedent.push(item);
                } else {
                    consequent.push(item);
                }
            }
            let (a, c) = (index_of(&antecedent)?, index_of(&consequent)?);
            let confidence = to_f64(frequent.support[id]) / to_f64(frequent.support[a]);
            if confidence >= min_confidence {
                rules.push(Rule {
                    antecedent: a,
                    consequent: c,
                    support: frequent.support[id],
                    confidence,
                    lift: confidence / (to_f64(frequent.support[c]) / total),
                });
            }
        }
    }
    Ok(rules)
}

#[allow(clippy::cast_precision_loss)] // los conteos caben de sobra en 2^53
fn to_f64(n: usize) -> f64 {
    n as f64
}

#[cfg(test)]
mod tests {
    use super::{
        FrequentItemsets, MAX_RULE_ITEMS, Transactions, association_rules, frequent_itemsets,
    };
    use geoda_core::{Column, Error, PrimitiveBuilder};

    fn ints(values: &[i64]) -> Column {
        let mut b = PrimitiveBuilder::<i64>::new();
        for &v in values {
            b.append(v);
        }
        b.finish().unwrap()
    }

    fn example() -> Transactions {
        Transactions::from_lists(&[vec![1, 2, 3], vec![1, 2], vec![1, 3], vec![2, 3]]).unwrap()
    }

    #[test]
    fn el_ejemplo_de_la_historia() {
        let result = frequent_itemsets(&example(), 2).unwrap();
        assert_eq!(
            result.itemsets,
            [
                vec![1],
                vec![2],
                vec![3],
                vec![1, 2],
                vec![1, 3],
                vec![2, 3]
            ]
        );
        assert_eq!(result.support, [3, 3, 3, 2, 2, 2]);
        assert_eq!(result.transactions, 4);
    }

    #[test]
    fn reglas_con_confianza_y_lift() {
        let frequent = frequent_itemsets(&example(), 2).unwrap();
        let rules = association_rules(&frequent, 0.6).unwrap();
        assert_eq!(rules.len(), 6);
        let rule = &rules[0];
        assert_eq!(frequent.itemsets[rule.antecedent], [1]);
        assert_eq!(frequent.itemsets[rule.consequent], [2]);
        assert_eq!(rule.support, 2);
        assert!((rule.confidence - 2.0 / 3.0).abs() < 1e-12);
        assert!((rule.lift - 8.0 / 9.0).abs() < 1e-12);
        assert_eq!(association_rules(&frequent, 0.7).unwrap().len(), 0);
    }

    #[test]
    fn un_item_repetido_cuenta_una_vez() {
        let t = Transactions::from_lists(&[vec![5, 5, 5], vec![5]]).unwrap();
        assert_eq!(frequent_itemsets(&t, 2).unwrap().support, [2]);
    }

    #[test]
    fn soporte_mayor_que_las_transacciones() {
        let result = frequent_itemsets(&example(), 5).unwrap();
        assert_eq!(result.itemsets.len(), 0);
    }

    #[test]
    fn argumentos_invalidos() {
        assert!(matches!(
            frequent_itemsets(&example(), 0),
            Err(Error::InvalidArgument(_))
        ));
        let frequent = frequent_itemsets(&example(), 2).unwrap();
        for bad in [0.0, -0.1, 1.5, f64::NAN] {
            assert!(matches!(
                association_rules(&frequent, bad),
                Err(Error::InvalidArgument(_))
            ));
        }
        assert!(matches!(
            Transactions::from_lists(&[vec![-1]]),
            Err(Error::InvalidArgument(_))
        ));
    }

    #[test]
    fn formato_largo() {
        let t = Transactions::from_columns(&ints(&[0, 0, 1, 1, 2, 2]), &ints(&[1, 2, 1, 2, 2, 3]))
            .unwrap();
        assert_eq!(t.len(), 3);
        let result = frequent_itemsets(&t, 2).unwrap();
        assert_eq!(result.itemsets, [vec![1], vec![2], vec![1, 2]]);
        assert!(matches!(
            Transactions::from_columns(&ints(&[0]), &ints(&[1, 2])),
            Err(Error::LengthMismatch(_))
        ));
        assert!(matches!(
            Transactions::from_columns(&ints(&[-1]), &ints(&[1])),
            Err(Error::InvalidArgument(_))
        ));
        let mut with_null = PrimitiveBuilder::<i64>::new();
        with_null.append_null();
        assert!(matches!(
            Transactions::from_columns(&with_null.finish().unwrap(), &ints(&[1])),
            Err(Error::InvalidArgument(_))
        ));
    }

    #[test]
    fn un_conjunto_enorme_no_agota_la_memoria() {
        // Armado a mano: calcularlo con Apriori daria 2^21 conjuntos.
        let big: Vec<i64> = (0..i64::try_from(MAX_RULE_ITEMS + 1).unwrap()).collect();
        let frequent = FrequentItemsets {
            itemsets: vec![big],
            support: vec![2],
            transactions: 2,
        };
        assert!(matches!(
            association_rules(&frequent, 0.5),
            Err(Error::InvalidArgument(_))
        ));
    }

    /// Oraculo: cuenta el soporte de todos los subconjuntos posibles de los items.
    fn brute(lists: &[Vec<i64>], universe: i64, min_support: usize) -> FrequentItemsets {
        let mut found: Vec<(Vec<i64>, usize)> = Vec::new();
        for mask in 1u32..(1 << universe) {
            let set: Vec<i64> = (0..universe).filter(|&i| mask >> i & 1 == 1).collect();
            let count = lists
                .iter()
                .filter(|l| set.iter().all(|i| l.contains(i)))
                .count();
            if count >= min_support {
                found.push((set, count));
            }
        }
        found.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then_with(|| a.0.cmp(&b.0)));
        FrequentItemsets {
            itemsets: found.iter().map(|f| f.0.clone()).collect(),
            support: found.iter().map(|f| f.1).collect(),
            transactions: lists.len(),
        }
    }

    #[test]
    fn coincide_con_el_conteo_por_fuerza_bruta() {
        let mut state: u64 = 0xA9;
        let mut next = move |bound: u64| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) % bound
        };
        for _ in 0..30 {
            let rows = 5 + usize::try_from(next(25)).unwrap();
            let lists: Vec<Vec<i64>> = (0..rows)
                .map(|_| (0..7).filter(|_| next(100) < 45).map(i64::from).collect())
                .collect();
            let transactions = Transactions::from_lists(&lists).unwrap();
            for min_support in 1..=4 {
                let got = frequent_itemsets(&transactions, min_support).unwrap();
                let want = brute(&lists, 7, min_support);
                assert_eq!(got, want, "min_support {min_support}, datos {lists:?}");
                // Propiedad de Apriori: todo subconjunto de un frecuente es frecuente.
                let rules = association_rules(&got, 0.01).unwrap();
                for rule in rules {
                    assert!(rule.confidence > 0.0 && rule.confidence <= 1.0);
                    assert!(rule.lift > 0.0);
                }
            }
        }
    }

    #[test]
    fn es_determinista() {
        let first = frequent_itemsets(&example(), 2).unwrap();
        for _ in 0..5 {
            assert_eq!(frequent_itemsets(&example(), 2).unwrap(), first);
        }
    }
}
