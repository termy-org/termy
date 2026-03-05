cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.44"
  sha256 arm:   "1179936b1c39aa72aadbd730b619020c0654202a145da44ae03ecfa9bcaf2f54",
         intel: "79b85d4770a2c16e0818a90e13a178a903d65856ac1ce70a2f49d0a95ce68189"

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
