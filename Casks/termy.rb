cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.60"
  sha256 arm:   "d5f71cbfddcc0eac1a1df2705400ce8dcca60587a2a17bb3df0c045f8414c7dd",
         intel: "baf7b892af3981761dd3e95c5f67c02afd26938a60315ff4cb5066293ed46a1e"

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
