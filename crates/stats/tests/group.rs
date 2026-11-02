//! Pruebas de los agregados por grupo (HU-024).

use geoda_algo::Value;
use geoda_core::{BoolBuilder, Column, Error, PrimitiveBuilder, Utf8Builder};
use geoda_stats::{
    Count, Max, Mean, Min, Number, Sum, Variance, group_aggregate, mean_column, sum_column,
    variance_column,
};

fn ints(values: &[i64]) -> Column {
    let mut b = PrimitiveBuilder::<i64>::new();
    for &v in values {
        b.append(v);
    }
    b.finish().unwrap()
}

fn opt_ints(values: &[Option<i64>]) -> Column {
    let mut b = PrimitiveBuilder::<i64>::new();
    for v in values {
        match v {
            Some(v) => b.append(*v),
            None => b.append_null(),
        }
    }
    b.finish().unwrap()
}

fn strings(values: &[Option<&str>]) -> Column {
    let mut b = Utf8Builder::new();
    for v in values {
        match v {
            Some(v) => b.append(v).unwrap(),
            None => b.append_null(),
        }
    }
    b.finish().unwrap()
}

fn text(s: &str) -> Value {
    Value::Utf8(s.to_string())
}

fn abab() -> (Column, Column) {
    let keys = strings(&[Some("a"), Some("b"), Some("a"), Some("b"), Some("a")]);
    (keys, ints(&[1, 2, 3, 4, 5]))
}

#[test]
fn sum_by_group_in_first_appearance_order() {
    let (keys, values) = abab();
    let got = group_aggregate(&Sum::new(values.dtype()).unwrap(), &keys, &values).unwrap();
    assert_eq!(got.keys, vec![Some(text("a")), Some(text("b"))]);
    assert_eq!(got.results, vec![Number::Int(9), Number::Int(6)]);
}

#[test]
fn every_aggregate_matches_applying_it_to_the_rows_of_the_group() {
    let (keys, values) = abab();
    let dtype = values.dtype();
    let a = ints(&[1, 3, 5]);
    let b = ints(&[2, 4]);

    let count = group_aggregate(&Count::new(dtype).unwrap(), &keys, &values).unwrap();
    assert_eq!(count.results, vec![3, 2]);

    let mean = group_aggregate(&Mean::new(dtype).unwrap(), &keys, &values).unwrap();
    assert_eq!(
        mean.results,
        vec![mean_column(&a).unwrap(), mean_column(&b).unwrap()]
    );

    let min = group_aggregate(&Min::new(dtype).unwrap(), &keys, &values).unwrap();
    assert_eq!(min.results, vec![Number::Int(1), Number::Int(2)]);
    let max = group_aggregate(&Max::new(dtype).unwrap(), &keys, &values).unwrap();
    assert_eq!(max.results, vec![Number::Int(5), Number::Int(4)]);

    let var = group_aggregate(&Variance::new(dtype, 1).unwrap(), &keys, &values).unwrap();
    assert_eq!(
        var.results[0].to_bits(),
        variance_column(&a, 1).unwrap().to_bits()
    );
    assert_eq!(
        var.results[1].to_bits(),
        variance_column(&b, 1).unwrap().to_bits()
    );
    assert_eq!(sum_column(&a).unwrap(), Number::Int(9));
}

#[test]
fn null_keys_form_their_own_group() {
    let keys = strings(&[Some("a"), None, Some("a"), None]);
    let values = ints(&[1, 10, 2, 20]);
    let got = group_aggregate(&Sum::new(values.dtype()).unwrap(), &keys, &values).unwrap();
    assert_eq!(got.keys, vec![Some(text("a")), None]);
    assert_eq!(got.results, vec![Number::Int(3), Number::Int(30)]);
}

#[test]
fn float_keys_nan_is_one_group_and_signed_zeros_are_two() {
    let mut b = PrimitiveBuilder::<f64>::new();
    for v in [f64::NAN, 0.0, -0.0, -f64::NAN, 0.0] {
        b.append(v);
    }
    let keys = b.finish().unwrap();
    let values = ints(&[1, 2, 4, 8, 16]);
    let got = group_aggregate(&Sum::new(values.dtype()).unwrap(), &keys, &values).unwrap();
    assert_eq!(got.keys.len(), 3);
    assert_eq!(
        got.results,
        vec![Number::Int(9), Number::Int(18), Number::Int(4)]
    );
    let Some(Value::Float64(first)) = got.keys[0] else {
        panic!("la primera clave debia ser Float64");
    };
    assert!(first.is_nan());
    let Some(Value::Float64(zero)) = got.keys[1] else {
        panic!("la segunda clave debia ser Float64");
    };
    assert_eq!(zero.to_bits(), 0.0_f64.to_bits());
    let Some(Value::Float64(neg_zero)) = got.keys[2] else {
        panic!("la tercera clave debia ser Float64");
    };
    assert_eq!(neg_zero.to_bits(), (-0.0_f64).to_bits());
}

#[test]
fn null_values_are_ignored_and_an_all_null_group_follows_the_aggregate() {
    let keys = strings(&[Some("a"), Some("a"), Some("b")]);
    let values = opt_ints(&[Some(4), None, None]);
    let dtype = values.dtype();

    let sum = group_aggregate(&Sum::new(dtype).unwrap(), &keys, &values).unwrap();
    assert_eq!(sum.results, vec![Number::Int(4), Number::Int(0)]);
    let count = group_aggregate(&Count::new(dtype).unwrap(), &keys, &values).unwrap();
    assert_eq!(count.results, vec![1, 0]);

    let mean = group_aggregate(&Mean::new(dtype).unwrap(), &keys, &values);
    assert!(matches!(mean, Err(Error::InvalidArgument(_))));
    let min = group_aggregate(&Min::new(dtype).unwrap(), &keys, &values);
    assert!(matches!(min, Err(Error::InvalidArgument(_))));
}

#[test]
fn different_lengths_are_a_length_mismatch() {
    let keys = strings(&[Some("a"), Some("b")]);
    let values = ints(&[1, 2, 3]);
    let got = group_aggregate(&Count::new(values.dtype()).unwrap(), &keys, &values);
    assert!(matches!(got, Err(Error::LengthMismatch(_))));
}

#[test]
fn all_five_key_types_work_and_the_empty_string_is_a_group() {
    let values = ints(&[1, 2, 3, 4]);
    let sum = Sum::new(values.dtype()).unwrap();

    let mut b = PrimitiveBuilder::<i32>::new();
    for v in [7, 7, 8, 7] {
        b.append(v);
    }
    let got = group_aggregate(&sum, &b.finish().unwrap(), &values).unwrap();
    assert_eq!(got.keys, vec![Some(Value::Int32(7)), Some(Value::Int32(8))]);
    assert_eq!(got.results, vec![Number::Int(7), Number::Int(3)]);

    let got = group_aggregate(&sum, &ints(&[5, 5, 6, 6]), &values).unwrap();
    assert_eq!(got.keys, vec![Some(Value::Int64(5)), Some(Value::Int64(6))]);
    assert_eq!(got.results, vec![Number::Int(3), Number::Int(7)]);

    let mut b = BoolBuilder::new();
    for v in [true, false, true, false] {
        b.append(v);
    }
    let got = group_aggregate(&sum, &b.finish().unwrap(), &values).unwrap();
    assert_eq!(
        got.keys,
        vec![Some(Value::Bool(true)), Some(Value::Bool(false))]
    );
    assert_eq!(got.results, vec![Number::Int(4), Number::Int(6)]);

    let keys = strings(&[Some(""), Some("x"), Some(""), Some("x")]);
    let got = group_aggregate(&sum, &keys, &values).unwrap();
    assert_eq!(got.keys, vec![Some(text("")), Some(text("x"))]);
    assert_eq!(got.results, vec![Number::Int(4), Number::Int(6)]);
}

#[test]
fn bool_and_utf8_values_are_unsupported() {
    let keys = ints(&[1]);
    let values = strings(&[Some("a")]);
    let got = group_aggregate(
        &Count::new(geoda_core::DataType::Int64).unwrap(),
        &keys,
        &values,
    );
    assert!(matches!(got, Err(Error::Unsupported(_))));
}

/// Generador congruencial simple: la semilla fija hace la prueba reproducible.
fn next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state >> 33
}

#[test]
fn matches_a_naive_grouping_in_a_thousand_random_cases() {
    let mut seed = 7_u64;
    for case in 0..1000 {
        let n = (next(&mut seed) % 80) as usize;
        // Mitad de los casos con pocos grupos, mitad con muchos.
        let groups = if case % 2 == 0 { 3 } else { 60 };
        let key_data: Vec<Option<i64>> = (0..n)
            .map(|_| match next(&mut seed) % 8 {
                0 => None,
                _ => Some(i64::try_from(next(&mut seed) % groups).unwrap()),
            })
            .collect();
        let value_data: Vec<Option<i64>> = (0..n)
            .map(|_| match next(&mut seed) % 6 {
                0 => None,
                _ => Some(i64::try_from(next(&mut seed) % 1001).unwrap() - 500),
            })
            .collect();
        let keys = opt_ints(&key_data);
        let values = opt_ints(&value_data);

        // Ingenuo O(n * g): claves distintas por primera aparicion y una pasada por cada una.
        let mut distinct: Vec<Option<i64>> = Vec::new();
        for k in &key_data {
            if !distinct.contains(k) {
                distinct.push(*k);
            }
        }
        let rows_of = |k: &Option<i64>| -> Vec<i64> {
            key_data
                .iter()
                .zip(&value_data)
                .filter(|(kk, _)| *kk == k)
                .filter_map(|(_, v)| *v)
                .collect()
        };

        let sum = group_aggregate(&Sum::new(values.dtype()).unwrap(), &keys, &values).unwrap();
        let count = group_aggregate(&Count::new(values.dtype()).unwrap(), &keys, &values).unwrap();
        let expected_keys: Vec<Option<Value>> =
            distinct.iter().map(|k| k.map(Value::Int64)).collect();
        assert_eq!(sum.keys, expected_keys, "caso {case}");
        assert_eq!(count.keys, expected_keys, "caso {case}");
        let expected_sum: Vec<Number> = distinct
            .iter()
            .map(|k| Number::Int(rows_of(k).iter().sum()))
            .collect();
        let expected_count: Vec<usize> = distinct.iter().map(|k| rows_of(k).len()).collect();
        assert_eq!(sum.results, expected_sum, "caso {case}");
        assert_eq!(count.results, expected_count, "caso {case}");
    }
}
