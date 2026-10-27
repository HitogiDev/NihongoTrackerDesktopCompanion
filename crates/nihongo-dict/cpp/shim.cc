#include "shim.h"

#include <string>

#include "hoshidicts.h"

#ifdef NT_DICT_BRIDGE_HEADER
#include NT_DICT_BRIDGE_HEADER
#else
#include "nihongo-dict/src/lib.rs.h"
#endif

namespace ntdict {
namespace {

rust::String to_rust(const std::string& value) { return rust::String(value); }

}  // namespace

struct Engine::Impl {
  DictionaryQuery query;
  Deinflector deinflector;
  Lookup lookup;

  Impl() : query(), deinflector(), lookup(query, deinflector) {}
};

Engine::Engine() : impl_(std::make_unique<Impl>()) {}
Engine::~Engine() = default;

void Engine::add_term_dict(rust::Str path) { impl_->query.add_term_dict(std::string(path)); }
void Engine::add_freq_dict(rust::Str path) { impl_->query.add_freq_dict(std::string(path)); }
void Engine::add_pitch_dict(rust::Str path) { impl_->query.add_pitch_dict(std::string(path)); }

rust::Vec<LookupRow> Engine::lookup(rust::Str text, int32_t max_results) const {
  rust::Vec<LookupRow> output;
  for (const auto& result : impl_->lookup.lookup(std::string(text), max_results)) {
    rust::Vec<rust::String> process;
    for (const auto& step : result.trace) {
      process.push_back(to_rust(step.name));
    }

    rust::Vec<Gloss> glossaries;
    std::string definition_tags;
    std::string term_tags;
    for (const auto& gloss : result.term.glossaries) {
      if (!gloss.definition_tags.empty()) {
        if (!definition_tags.empty()) definition_tags += " ";
        definition_tags += gloss.definition_tags;
      }
      if (!gloss.term_tags.empty()) {
        if (!term_tags.empty()) term_tags += " ";
        term_tags += gloss.term_tags;
      }
      glossaries.push_back(Gloss{to_rust(gloss.dict_name), to_rust(gloss.glossary)});
    }

    rust::Vec<FreqHit> frequencies;
    for (const auto& entry : result.term.frequencies) {
      for (const auto& frequency : entry.frequencies) {
        frequencies.push_back(FreqHit{to_rust(entry.dict_name), to_rust(frequency.display_value), frequency.value});
      }
    }

    output.push_back(LookupRow{
        to_rust(result.matched),
        to_rust(result.deinflected),
        std::move(process),
        to_rust(result.term.expression),
        to_rust(result.term.reading),
        to_rust(definition_tags),
        to_rust(result.term.rules),
        to_rust(term_tags),
        result.term.score,
        std::move(glossaries),
        std::move(frequencies),
    });
  }
  return output;
}

std::unique_ptr<Engine> new_engine() { return std::make_unique<Engine>(); }

ImportSummary import_dictionary(rust::Str zip_path, rust::Str out_dir) {
  const ImportResult result = dictionary_importer::import(std::string(zip_path), std::string(out_dir), false);
  return ImportSummary{
      result.success,
      to_rust(result.title),
      static_cast<uint64_t>(result.summary.counts.terms.total),
      static_cast<uint64_t>(result.summary.counts.termMeta.at("total")),
      static_cast<uint64_t>(result.summary.counts.termMeta.contains("freq") ? result.summary.counts.termMeta.at("freq") : 0),
      static_cast<uint64_t>(result.summary.counts.termMeta.contains("pitch") ? result.summary.counts.termMeta.at("pitch") : 0),
      to_rust(result.error),
  };
}

}  // namespace ntdict
