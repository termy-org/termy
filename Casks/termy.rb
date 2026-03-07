cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.46"
  sha256 arm:   "9e909a8e8ac3e6e3c820bcf8e571756825b1579f221b418a434101c86720dcaf",
         intel: "46d3ef42a5781f808d9bc7b60dd1c0ed37e8dd40de0eb4ddf0b8afae1fdc8b90"

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
