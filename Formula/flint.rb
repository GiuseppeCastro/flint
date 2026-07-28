# Formula for a tap (e.g. `GiuseppeCastro/homebrew-flint`), not homebrew-core.
#
# After cutting a GitHub release (pushing a `vX.Y.Z` tag builds and uploads
# the two tarballs below via .github/workflows/release.yml), update `version`
# and both `sha256` values from the release's `.sha256` sidecar files:
#   shasum -a 256 -c flint-vX.Y.Z-aarch64-apple-darwin.tar.gz.sha256
class Flint < Formula
  desc "Fast, local-first terminal autocomplete and history for Zsh"
  homepage "https://github.com/GiuseppeCastro/flint"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/GiuseppeCastro/flint/releases/download/v#{version}/flint-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "14b9901546ae446d403dc93e96fca8cb0e174117348a139355924bae9f49a650"
    end
    on_intel do
      url "https://github.com/GiuseppeCastro/flint/releases/download/v#{version}/flint-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "934a7e25d8cc466ba056de6582950beff9e889300545d3ae47dd8e4915a6f668"
    end
  end

  def install
    bin.install "flint"
  end

  test do
    assert_match "flint #{version}", shell_output("#{bin}/flint --version")
    assert_match "_flint_precmd", shell_output("#{bin}/flint init zsh")
  end
end
