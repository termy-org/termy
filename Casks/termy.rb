cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.35"
  sha256 arm:   "ce797fc07ebd6341241c207557285aca44dc190c3c7e5b06e17e040627d3b18a",
         intel: "bcc4af1739334e9627146fb837e3a51305b6a36c4fff512258714c5b73e6c2e5"

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
