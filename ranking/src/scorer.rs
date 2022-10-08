use std::borrow::Borrow;

#[derive(Debug, Clone, Default)]
pub struct Score {
    scores: Vec<(f64, f64, &'static str)>,
    total: f64,
}

#[derive(Debug, Default)]
pub struct ScoreAdj<'a> {
    score: Option<&'a mut f64>,
}

impl Score {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    /// Add score if it has the given property
    pub fn has(&mut self, for_what: &'static str, score: u32, has_it: bool) -> ScoreAdj<'_> {
        self.score_f(for_what, score as f64, if has_it { score as f64 } else { 0. })
    }

    #[inline]
    /// Add this much score, up to the max
    pub fn n(&mut self, for_what: &'static str, max_score: u32, n: impl Into<i64>) -> ScoreAdj<'_> {
        self.score_f(for_what, max_score as f64, n.into() as f64)
    }

    /// Add `max_score` * `n` where n is in 0..1
    #[track_caller]
    pub fn frac(&mut self, for_what: &'static str, max_score: u32, n: impl Into<f64>) -> ScoreAdj<'_> {
        let n = n.into();
        assert!(n >= 0.);
        assert!(n <= 1.);
        let max_score = max_score as f64;
        self.score_f(for_what, max_score, n * max_score)
    }

    #[inline]
    /// Add `n` of `max_score` points
    pub fn score_f(&mut self, for_what: &'static str, max_score: f64, n: impl Into<f64>) -> ScoreAdj<'_> {
        let n = n.into();
        self.total += max_score;
        self.scores.push((n.max(0.), max_score, for_what));
        ScoreAdj { score: self.scores.last_mut().map(|(s, ..)| s) }
    }

    /// Start a new group of scores, and `max_score` is the max total score of the group
    pub fn group(&mut self, for_what: &'static str, max_score: u32, group: impl Borrow<Score>) -> ScoreAdj<'_> {
        self.frac(for_what, max_score, group.borrow().total())
    }

    /// Get total score
    pub fn total(&self) -> f64 {
        let sum = self.scores.iter().map(|&(v, limit, _)| v.max(0.).min(limit)).sum::<f64>();
        sum / self.total as f64
    }
}

impl<'a> ScoreAdj<'a> {
    pub fn mul(&mut self, by: f64) {
        self.adj(|n| n * by)
    }

    pub fn adj(&mut self, adj_with: impl FnOnce(f64) -> f64) {
        if let Some(s) = self.score.as_mut() {
            **s = adj_with(**s);
        }
    }
}

#[test]
fn scores() {
    let mut s1 = Score::new();
    s1.has("foo", 5, true);
    assert_eq!(1., s1.total());
    s1.has("bar", 15, false);
    assert!(s1.total() <= 0.26);
    assert!(s1.total() >= 0.24);
    let mut s2 = Score::new();
    s2.n("baz", 10, 5);
    s2.frac("baz2", 28, 0.5);
    assert!(s2.total() >= 0.49);
    assert!(s2.total() <= 0.51);
    let mut s3 = Score::new();
    s3.group("prev", 100, s1);
    s3.group("prev", 10, s2);
    assert!(s3.total() >= 0.26);
    assert!(s3.total() <= 0.28);
}
