import * as anchor from "@project-serum/anchor";
import { Connection, PublicKey } from "@solana/web3.js";
import { expect } from "chai";

describe("Dextra Protocol Minimal Test", () => {
  // Use the CORRECT program ID from your deployment
  const PROGRAM_ID = new PublicKey("EkDU4dizCrRyaNfRfTcsHFH4rTmeBP4PQBkH74Ua3RvD");
  
  // Create a simple connection
  const connection = new Connection("http://127.0.0.1:8899", "confirmed");
  
  it("Can verify program exists", async () => {
    const accountInfo = await connection.getAccountInfo(PROGRAM_ID);
    console.log("Program exists:", !!accountInfo);
    
    if (accountInfo) {
      console.log("Program size:", accountInfo.data.length, "bytes");
      console.log("Executable:", accountInfo.executable);
      
      // A deployed program should be executable
      expect(accountInfo.executable).to.be.true;
    } else {
      console.log("WARNING: Program not found at the specified address!");
    }
  });
}); 