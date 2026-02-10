use big_brain::{
    measures::WeightedMeasure,
    prelude::{Evaluator, Measure, Score, SigmoidEvaluator, WeightedProduct},
};

fn approx_eq(a: f32, b: f32, eps: f32) {
    assert!(
        (a - b).abs() <= eps,
        "expected {a} to be within {eps} of {b}"
    );
}

#[test]
fn weighted_product_multiplies_inputs() {
    let mut a = Score::default();
    let mut b = Score::default();
    a.set(0.5);
    b.set(0.4);

    let result = WeightedProduct.calculate(vec![(&a, 1.0), (&b, 0.5)]);
    approx_eq(result, 0.1, 1e-6);
}

#[test]
fn weighted_measure_returns_zero_for_zero_weight_sum() {
    let mut a = Score::default();
    a.set(0.7);

    let result = WeightedMeasure.calculate(vec![(&a, 0.0)]);
    approx_eq(result, 0.0, 1e-6);
}

#[test]
fn sigmoid_evaluator_hits_range_endpoints() {
    let evaluator = SigmoidEvaluator::new_ranged(0.0, 10.0, 20.0);

    approx_eq(evaluator.evaluate(10.0), 0.0, 1e-6);
    approx_eq(evaluator.evaluate(20.0), 1.0, 1e-6);
}
