#pragma once

#include <memory>

#include "rust/cxx.h"

namespace ntdict {

struct ImportSummary;
struct LookupRow;

class Engine {
 public:
  Engine();
  ~Engine();

  void add_term_dict(rust::Str path);
  void add_freq_dict(rust::Str path);
  void add_pitch_dict(rust::Str path);
  rust::Vec<LookupRow> lookup(rust::Str text, int32_t max_results) const;

 private:
  struct Impl;
  std::unique_ptr<Impl> impl_;
};

std::unique_ptr<Engine> new_engine();
ImportSummary import_dictionary(rust::Str zip_path, rust::Str out_dir);

}  // namespace ntdict
