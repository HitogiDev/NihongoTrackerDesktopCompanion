// cxx shim over hoshidicts.
//
// hoshidicts' own API is idiomatic C++ (nested structs holding std::vector and
// std::unordered_map), and cxx cannot bind that directly. This translates it
// into the flat shared structs declared in `src/dict.rs`.
#pragma once

#include <memory>
#include <string>

#include "rust/cxx.h"

namespace ntdict {

struct ImportSummary;
struct LookupRow;

// Owns the query index, the deconjugator and the lookup wired together. They
// have to outlive each other in that order, which is exactly the sort of thing
// cxx cannot express on its own.
class Engine {
 public:
  Engine();
  ~Engine();

  void add_term_dict(rust::Str path);
  void add_freq_dict(rust::Str path);
  rust::Vec<LookupRow> lookup(rust::Str text, int32_t max_results) const;

 private:
  struct Impl;
  std::unique_ptr<Impl> impl_;
};

std::unique_ptr<Engine> new_engine();
ImportSummary import_dictionary(rust::Str zip_path, rust::Str out_dir);

}  // namespace ntdict
