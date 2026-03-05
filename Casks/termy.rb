cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.43"
  sha256 arm:   "502758373c1da3a7e4f5b4f7452882d030fb13b19452132f79cda915cb77595e",
         intel: "4cef60ba0c55892961f39e8bb4c6a399334d4fd6c7783a6a5f398165fc7998f0"

  url "https://github.com/lassejlv/termy/releases/download/v#{version}/Termy-v#{version}-macos-#{arch}.dmg"
  name "Termy"
  desc "Minimal GPU-powered terminal written in Rust"
  homepage "https://github.com/lassejlv/termy"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :big_sur"

  app "Termy.app"
end
