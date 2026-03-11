cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.50"
  sha256 arm:   "35f2aceeff6d2cbc5c440861d2adae2fcb7a16654ac11770a1ff0713cf135b78",
         intel: "fba85113aefa32ad5a278b33da1e7e690630579e41c12a04e96520049c6191e9"

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
