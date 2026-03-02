cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.36"
  sha256 arm:   "0d20ec437acaa1a27fc55dd74eda1fbb453d74168477bda0842b3f55a7070f99",
         intel: "08d83bc34a9294b400a7bb0f014f40e08c02f2792a003271fff663b3859d9f95"

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
