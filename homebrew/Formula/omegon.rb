# typed: false
# frozen_string_literal: true

class Omegon < Formula
  desc "Terminal-native AI agent harness with an independent recovery companion"
  homepage "https://omegon.styrene.io"
  license "BUSL-1.1"
  version "0.15.11"

  LINUX_MIN_GLIBC = Version.new("2.39")

  on_macos do
    on_arm do
      url "https://github.com/styrene-lab/omegon/releases/download/v#{version}/omegon-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "9e4dbcf5875d09d17cfaf9b134d23755379dc4f07c046cb9f6b368373787eda1"
    end

    on_intel do
      url "https://github.com/styrene-lab/omegon/releases/download/v#{version}/omegon-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "d4354a09fef984110728a6ab06a64942a6cfbbbf03173a1951975a68b106d37d"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/styrene-lab/omegon/releases/download/v#{version}/omegon-#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "525ce0abb34e2e9ecf036d33347104eb1009603ff48ebf01acd91102102d1344"
    end

    on_intel do
      url "https://github.com/styrene-lab/omegon/releases/download/v#{version}/omegon-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "428b3a8260bd38afea00b3fcac798a009ff4ebe9015bd09780e29d71e7a4ccc4"
    end
  end

  def install
    check_linux_glibc_requirement!
    bin.install "omegon"
    bin.install "omegon-maintain"
    bin.install_symlink "omegon" => "om"
  end

  def caveats
    <<~EOS
      Three entrypoints are installed:
        omegon  # full harness
        om      # slim harness (same binary, alias-based mode)
        omegon-maintain  # offline maintenance and recovery companion

      To get started:
        export ANTHROPIC_API_KEY="sk-ant-..."
        om

      Or authenticate with Claude Pro/Max:
        omegon auth login anthropic

      Documentation: https://omegon.styrene.io/docs/
    EOS
  end

  test do
    assert_match "omegon", shell_output("#{bin}/omegon --version")
    assert_match "omegon-maintain", shell_output("#{bin}/omegon-maintain --version")
    assert_match '"status":"success"', shell_output("#{bin}/omegon-maintain --json identity")
  end

  private

  def check_linux_glibc_requirement!
    return unless OS.linux?

    glibc = detect_glibc_version
    return if glibc && glibc >= LINUX_MIN_GLIBC

    detected = glibc ? glibc.to_s : "unknown"
    odie <<~EOS
      Omegon's Linux Homebrew binary currently requires glibc >= #{LINUX_MIN_GLIBC}.
      Detected glibc: #{detected}

      Homebrew on Linux does not upgrade your host glibc to satisfy Omegon's runtime ABI.
      Use a newer Linux distribution, container, or VM with a compatible glibc baseline.

      See: https://omegon.styrene.io/docs/install/
    EOS
  end

  def detect_glibc_version
    output = Utils.safe_popen_read("ldd", "--version")
    first_line = output.lines.first.to_s
    match = first_line.match(/(\d+\.\d+)/)
    match ? Version.new(match[1]) : nil
  rescue Errno::ENOENT
    nil
  end
end
