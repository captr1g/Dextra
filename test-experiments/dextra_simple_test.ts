import * as anchor from "@project-serum/anchor";
import { Program } from "@project-serum/anchor";
import { expect } from "chai";

describe("Dextra Protocol - Structure Test", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  // Get program from Anchor.toml
  // Note: We're not using a type-safe version to avoid compilation issues
  const program = anchor.workspace.dextra as any;

  it("Has expected instruction functions", () => {
    // Check that core functions exist
    expect(program.methods.initialize).to.be.a("function");
    expect(program.methods.addPool).to.be.a("function");
    expect(program.methods.deposit).to.be.a("function");
    expect(program.methods.withdraw).to.be.a("function");
    expect(program.methods.claim).to.be.a("function");
    expect(program.methods.swap).to.be.a("function");
    
    // Admin functions
    expect(program.methods.updateRate).to.be.a("function");
    expect(program.methods.updateApy).to.be.a("function");
    expect(program.methods.updatePool).to.be.a("function");
    expect(program.methods.approve).to.be.a("function");
    expect(program.methods.masscall).to.be.a("function");
    
    // Getter functions
    expect(program.methods.getClaimable).to.be.a("function");
    expect(program.methods.getAvailableSumForWithdraw).to.be.a("function");
    
    console.log("✅ All core protocol functions exist");
  });

  it("Has expected account structures", () => {
    // Check that account definitions exist
    expect(program.account.protocol).to.be.an("object");
    expect(program.account.pool).to.be.an("object");
    expect(program.account.userInfo).to.be.an("object");
    
    console.log("✅ All account structures exist");
  });

  it("Prints program details", () => {
    // Print important details about the program
    console.log("Program ID:", program.programId.toString());
    
    // Print program instructions for reference
    console.log("\nAvailable Instructions:");
    const methods = Object.keys(program.methods).sort();
    methods.forEach(method => {
      console.log(`- ${method}`);
    });
    
    // Print account structures
    console.log("\nAccount Structures:");
    const accounts = Object.keys(program.account).sort();
    accounts.forEach(account => {
      console.log(`- ${account}`);
    });
  });
}); 