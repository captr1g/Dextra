import * as anchor from "@project-serum/anchor";
import { PublicKey, Connection, clusterApiUrl } from "@solana/web3.js";
import fs from 'fs';

describe("Basic Dextra Test", () => {
  // Use local connection
  const connection = new Connection("http://127.0.0.1:8899", "confirmed");
  
  // Load program ID and IDL directly
  const programId = new PublicKey("BhL4V5qTP33T3PyjRZbqh1ALSBmkoMMFGLv4Whrf315S");
  
  it("Can connect to program", async () => {
    // Simple connection test
    const accountInfo = await connection.getAccountInfo(programId);
    console.log("Program exists on chain:", !!accountInfo);
    
    if (!accountInfo) {
      console.log("WARNING: Program not deployed to this cluster");
    } else {
      console.log("Program size:", accountInfo.data.length);
    }
  });
  
  it("Can load IDL", async () => {
    try {
      // Load IDL directly from file
      const idlFile = fs.readFileSync('./target/idl/dextra.json', 'utf8');
      const idl = JSON.parse(idlFile);
      console.log("IDL loaded successfully");
      console.log("Instructions:", idl.instructions.map(i => i.name));
      
      // Create a wallet for provider
      const wallet = new anchor.Wallet(anchor.web3.Keypair.generate());
      
      // Create provider
      const provider = new anchor.AnchorProvider(
        connection,
        wallet,
        { commitment: "confirmed" }
      );
      
      // Create program without using workspace
      const program = new anchor.Program(idl, programId, provider);
      console.log("Program initialized successfully");
      
      // Just verify we can access basic program properties
      console.log("Program ID:", program.programId.toString());
      console.log("Program methods:", Object.keys(program.methods).length);
    } catch (err) {
      console.error("Error loading IDL:", err);
      throw err;
    }
  });
}); 