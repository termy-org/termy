cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.56"
  sha256 arm:   "4720b298e166243dc12b8241cb1001f9d53b5d10d2e83a1fbb712eba7db6c0d4",
         intel: "674bf4b487213c6cad485a0742aa9bd0ad206f2c842544124261c5f8d534785e"

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
