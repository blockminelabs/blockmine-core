import * as anchor from "@coral-xyz/anchor";
import { expect } from "chai";

describe("blockmine", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  it("boots the Anchor provider", async () => {
    expect(provider.connection.rpcEndpoint.length).to.be.greaterThan(0);
  });

  describe("protocol lifecycle", () => {
    it.skip("initializes protocol config and opens block zero", async () => {});
    it.skip("accepts a valid solution and pays the reward", async () => {});
    it.skip("rejects an invalid solution", async () => {});
    it.skip("rejects duplicate settlement after block rotation", async () => {});
    it.skip("records block history and opens the next block", async () => {});
    it.skip("adjusts difficulty on the configured interval", async () => {});
  });
});
