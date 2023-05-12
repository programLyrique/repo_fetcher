#! /usr/bin/Rscript

args <- commandArgs(trailingOnly=TRUE)

filename <- args[[1]]

cat("Reading repositories from ", filename, "\n")

stopifnot(file.exists(filename))

repos <- readLines(filename)

output <- c("repository", paste0("https://github.com/", repos, ".git"))
writeLines(output, paste0(filename, ".reps"))
