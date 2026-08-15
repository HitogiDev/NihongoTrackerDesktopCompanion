#include "shim.h"

#include <string>
#include <vector>

#include "hoshidicts.h"

// cxx names the generated header after the crate, so the path differs between
// the app crate and the standalone smoke test that links the same shim.
#ifdef NT_DICT_BRIDGE_HEADER
#include NT_DICT_BRIDGE_HEADER
#else
#include "nt-companion/src/dict.rs.h"
#endif

namespace ntdict {
namespace {

rust::String to_rust(const std::string& value) { return rust::String(value); }

}  // namespace

struct Engine::Impl {
  DictionaryQuery query;
  Deconjugator deconjugator;
  Lookup lookup;

  Impl() : query(), deconjugator(), lookup(query, deconjugator) {}
};

Engine::Engine() : impl_(std::make_unique<Impl>()) {}
Engine::~Engine() = default;

void Engine::add_term_dict(rust::Str path) { impl_->query.add_term_dict(std::string(path)); }

void Engine::add_freq_dict(rust::Str path) { impl_->query.add_freq_dict(std::string(path)); }

rust::Vec<LookupRow> Engine::lookup(rust::Str text, int32_t max_results) const {
  rust::Vec<LookupRow> out;
  for (const auto& result : impl_->lookup.lookup(std::string(text), max_results)) {
    rust::Vec<rust::String> process;
    for (const auto& step : result.process) {
      process.push_back(to_rust(step));
    }

    rust::Vec<Gloss> glossaries;
    for (const auto& gloss : result.term.glossaries) {
      glossaries.push_back(Gloss{to_rust(gloss.dict_name), to_rust(gloss.glossary)});
    }

    rust::Vec<FreqHit> frequencies;
    for (const auto& entry : result.term.frequencies) {
      for (const auto& freq : entry.frequencies) {
        frequencies.push_back(FreqHit{to_rust(entry.dict_name), to_rust(freq.display_value), freq.value});
      }
    }

    out.push_back(LookupRow{
        to_rust(result.matched),
        to_rust(result.deinflected),
        std::move(process),
        to_rust(result.term.expression),
        to_rust(result.term.reading),
        to_rust(result.term.rules),
        std::move(glossaries),
        std::move(frequencies),
    });
  }
  return out;
}

std::unique_ptr<Engine> new_engine() { return std::make_unique<Engine>(); }

ImportSummary import_dictionary(rust::Str zip_path, rust::Str out_dir) {
  const ImportResult result = dictionary_importer::import(std::string(zip_path), std::string(out_dir));

  rust::Vec<rust::String> errors;
  for (const auto& error : result.errors) {
    errors.push_back(to_rust(error));
  }

  return ImportSummary{
      result.success,
      to_rust(result.title),
      static_cast<uint64_t>(result.term_count),
      static_cast<uint64_t>(result.meta_count),
      std::move(errors),
  };
}

}  // namespace ntdict
