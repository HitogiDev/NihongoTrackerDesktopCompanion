//! manga-ocr's decoding: beam search as `transformers` runs it with the
//! model's generation config (`num_beams = 4`, `no_repeat_ngram_size = 3`,
//! `length_penalty = 2.0`, `early_stopping = true`, `max_length = 300`).
//!
//! Each step scores every running beam's next token (`log_softmax` of the
//! decoder's last logits), bans tokens that would repeat a 3-gram, adds the
//! beam's score and keeps the best `2 × beams` candidates. A candidate that is
//! the end token becomes a finished hypothesis (if it ranks among the first
//! `beams`), scored `sum_logprobs / generated_len ^ length_penalty`; the rest
//! become the next beams. With early stopping the search ends as soon as
//! `beams` hypotheses are finished. The decoder is a function here, so the
//! search is tested without a model.

pub struct Config {
    pub beams: usize,
    pub no_repeat_ngram: usize,
    pub length_penalty: f32,
    /// Tokens, start token included.
    pub max_length: usize,
    pub bos: i64,
    pub eos: i64,
}

impl Config {
    pub fn manga_ocr() -> Self {
        Config { beams: 4, no_repeat_ngram: 3, length_penalty: 2.0, max_length: 300, bos: super::text::BOS, eos: super::text::EOS }
    }
}

struct Hypotheses {
    capacity: usize,
    length_penalty: f32,
    /// `(score, tokens)`, start token included, end token not.
    list: Vec<(f32, Vec<i64>)>,
}

impl Hypotheses {
    fn add(&mut self, tokens: Vec<i64>, sum_logprobs: f32, generated: usize) {
        let score = sum_logprobs / (generated.max(1) as f32).powf(self.length_penalty);
        if self.list.len() < self.capacity {
            self.list.push((score, tokens));
            return;
        }
        let (worst, worst_score) = self
            .list
            .iter()
            .enumerate()
            .map(|(index, (score, _))| (index, *score))
            .fold((0, f32::INFINITY), |acc, item| if item.1 < acc.1 { item } else { acc });
        if score > worst_score {
            self.list[worst] = (score, tokens);
        }
    }

    fn best(self) -> Option<Vec<i64>> {
        self.list.into_iter().max_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, tokens)| tokens)
    }
}

/// Tokens that would complete an n-gram already in `tokens`.
fn banned(tokens: &[i64], n: usize) -> Vec<i64> {
    if n == 0 || tokens.len() + 1 < n {
        return Vec::new();
    }
    let prefix = &tokens[tokens.len() + 1 - n..];
    tokens.windows(n).filter(|gram| &gram[..n - 1] == prefix).map(|gram| gram[n - 1]).collect()
}

fn log_softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let sum: f32 = logits.iter().map(|x| (x - max).exp()).sum();
    let log_sum = sum.ln() + max;
    logits.iter().map(|x| x - log_sum).collect()
}

/// Runs the search. `step` gets the running beams (all the same length) and
/// returns, for each, the logits of its next token.
pub fn search<E>(config: &Config, mut step: impl FnMut(&[Vec<i64>]) -> Result<Vec<Vec<f32>>, E>) -> Result<Vec<i64>, E> {
    // Every beam starts as the same start token: only one is scored first,
    // like `transformers`, which gives the copies a score of -1e9.
    let mut running: Vec<(Vec<i64>, f32)> = vec![(vec![config.bos], 0.0)];
    let mut done = Hypotheses { capacity: config.beams, length_penalty: config.length_penalty, list: Vec::new() };
    let mut finished = false;

    while running[0].0.len() < config.max_length {
        let sequences: Vec<Vec<i64>> = running.iter().map(|(tokens, _)| tokens.clone()).collect();
        let logits = step(&sequences)?;
        let mut candidates: Vec<(f32, usize, i64)> = Vec::new();
        for (beam, ((tokens, score), logits)) in running.iter().zip(&logits).enumerate() {
            let mut scores = log_softmax(logits);
            for token in banned(tokens, config.no_repeat_ngram) {
                if let Some(slot) = usize::try_from(token).ok().and_then(|token| scores.get_mut(token)) {
                    *slot = f32::NEG_INFINITY;
                }
            }
            candidates.extend(scores.into_iter().enumerate().map(|(token, logprob)| (logprob + score, beam, token as i64)));
        }
        let keep = (2 * config.beams).min(candidates.len());
        if keep == 0 {
            break;
        }
        candidates.select_nth_unstable_by(keep - 1, |a, b| b.0.total_cmp(&a.0));
        candidates.truncate(keep);
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));

        let generated = running[0].0.len() - 1;
        let mut next = Vec::with_capacity(config.beams);
        for (rank, (score, beam, token)) in candidates.into_iter().enumerate() {
            if !score.is_finite() {
                break;
            }
            if token == config.eos {
                if rank < config.beams {
                    done.add(running[beam].0.clone(), score, generated);
                }
            } else {
                let mut tokens = running[beam].0.clone();
                tokens.push(token);
                next.push((tokens, score));
                if next.len() == config.beams {
                    break;
                }
            }
        }
        finished = done.list.len() >= config.beams;
        running = next;
        if finished || running.is_empty() {
            break;
        }
    }
    // Still running at the length limit: they count as they are.
    if !finished {
        for (tokens, score) in running {
            let generated = tokens.len() - 1;
            done.add(tokens, score, generated);
        }
    }
    Ok(done.best().unwrap_or_else(|| vec![config.bos]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(beams: usize) -> Config {
        Config { beams, no_repeat_ngram: 3, length_penalty: 2.0, max_length: 20, bos: 0, eos: 1 }
    }

    /// A model that wants to say `wanted`, then stop.
    fn spelling(wanted: &'static [i64], vocab: usize) -> impl FnMut(&[Vec<i64>]) -> Result<Vec<Vec<f32>>, ()> {
        move |beams| {
            Ok(beams
                .iter()
                .map(|tokens| {
                    let position = tokens.len() - 1;
                    let next = wanted.get(position).copied().unwrap_or(1);
                    (0..vocab as i64).map(|token| if token == next { 5.0 } else { 0.0 }).collect()
                })
                .collect())
        }
    }

    #[test]
    fn finds_what_the_model_says() {
        assert_eq!(search(&config(4), spelling(&[2, 3, 4], 6)).unwrap(), [0, 2, 3, 4]);
        assert_eq!(search(&config(1), spelling(&[5, 2], 6)).unwrap(), [0, 5, 2]);
    }

    #[test]
    fn never_repeats_a_trigram() {
        // The model loops "2 3 2 3 2 3…": the third "2 3" would repeat "3 2 3"'s
        // trigram, so the search has to leave the loop.
        let looping = |beams: &[Vec<i64>]| -> Result<Vec<Vec<f32>>, ()> {
            Ok(beams.iter().map(|tokens| {
                let next = if tokens.last() == Some(&2) { 3 } else { 2 };
                (0..5i64).map(|token| if token == next { 5.0 } else if token == 1 { 1.0 } else { 0.0 }).collect()
            }).collect())
        };
        let tokens = search(&config(2), looping).unwrap();
        for gram in tokens.windows(3) {
            assert_eq!(tokens.windows(3).filter(|other| *other == gram).count(), 1, "{tokens:?}");
        }
        assert!(tokens.len() < 20);
    }

    #[test]
    fn a_beam_can_beat_the_greedy_path() {
        // Greedy takes 2 (0.6) then is unsure (0.5/0.5); 3 (0.4) leads to a
        // certain end. Beam search keeps both and the second wins.
        let model = |beams: &[Vec<i64>]| -> Result<Vec<Vec<f32>>, ()> {
            Ok(beams.iter().map(|tokens| {
                let probs: [f32; 5] = match tokens.as_slice() {
                    [0] => [0.0, 0.0, 0.6, 0.4, 0.0],
                    [0, 2] => [0.0, 0.5, 0.0, 0.0, 0.5],
                    [0, 3] => [0.0, 1.0, 0.0, 0.0, 0.0],
                    _ => [0.0, 1.0, 0.0, 0.0, 0.0],
                };
                probs.iter().map(|p| p.max(1e-9).ln()).collect()
            }).collect())
        };
        let mut config = config(2);
        config.length_penalty = 0.0;
        assert_eq!(search(&config, model).unwrap(), [0, 3]);
    }

    #[test]
    fn stops_at_the_length_limit() {
        let never_ends = |beams: &[Vec<i64>]| -> Result<Vec<Vec<f32>>, ()> {
            Ok(beams.iter().map(|tokens| (0..50i64).map(|t| if t == 2 + tokens.len() as i64 { 9.0 } else { 0.0 }).collect()).collect())
        };
        let tokens = search(&Config { max_length: 6, ..config(2) }, never_ends).unwrap();
        assert_eq!(tokens.len(), 6);
    }

    #[test]
    fn bans_only_completions_of_seen_ngrams() {
        assert_eq!(banned(&[2, 3, 4, 2, 3], 3), [4]);
        assert!(banned(&[0, 2, 3, 2], 3).is_empty());
        assert!(banned(&[0, 2], 3).is_empty());
        assert_eq!(banned(&[7, 7], 2), [7]);
    }
}
