cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.59"
  sha256 arm:   "1cb27b568f1ac5821c039e3b765ce12484a07e16ab05d721f54d8a738d59a0e2",
         intel: "16616452e9571b2ed137779bff2dda87f199f5e61d792df25d4a95da495642a4"

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
