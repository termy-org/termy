cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.65"
  sha256 arm:   "9289be9951a5afc7d2c446eb0bb00730c4451f17c6792a10c6d08019cbbdcf47",
         intel: "0e3cd12d5f2712207d378a4519607b81087d66341fb1225869ee0ccc883e7a8d"

  url "https://github.com/termy-org/termy/releases/download/v#{version}/Termy-v#{version}-macos-#{arch}.dmg"
  name "Termy"
  desc "Minimal GPU-powered terminal written in Rust"
  homepage "https://termy.dev"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :big_sur"

  app "Termy.app"
end
