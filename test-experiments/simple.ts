import * as anchor from "@project-serum/anchor";

describe("Basic Connection Test", () => {
  it("Can connect to program", async () => {
    const programId = new anchor.web3.PublicKey("BhL4V5qTP33T3PyjRZbqh1ALSBmkoMMFGLv4Whrf315S");
    console.log("Program ID:", programId.toString());
    
    // Just check that the program exists on chain
    const connection = new anchor.web3.Connection("http://127.0.0.1:8899", "confirmed");
    const accountInfo = await connection.getAccountInfo(programId);
    console.log("Program exists:", !!accountInfo);
  });
}); 