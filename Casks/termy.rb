cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.61"
  sha256 arm:   "6e93581872410fea836d3ede1cf249d0a6d2f5c47c0206e2fa6dbe74d96c8fa5",
         intel: "88c61a4e1d4b59a427da9f5c18b4b06ca45ff5f7c2e93c5ad453bea880480319"

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
