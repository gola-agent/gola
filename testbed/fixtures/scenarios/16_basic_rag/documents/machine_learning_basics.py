"""
Machine Learning Fundamentals and Common Algorithms

This document covers the basics of machine learning, including key concepts,
algorithms, and practical applications.
"""

# Machine Learning Types

SUPERVISED_LEARNING = {
    "definition": "Learning with labeled training data",
    "algorithms": [
        "Linear Regression",
        "Logistic Regression", 
        "Decision Trees",
        "Random Forest",
        "Support Vector Machines (SVM)",
        "Neural Networks"
    ],
    "use_cases": [
        "Image classification",
        "Spam detection",
        "Price prediction",
        "Medical diagnosis"
    ]
}

UNSUPERVISED_LEARNING = {
    "definition": "Finding patterns in data without labels",
    "algorithms": [
        "K-means Clustering",
        "Hierarchical Clustering",
        "Principal Component Analysis (PCA)",
        "DBSCAN",
        "Gaussian Mixture Models"
    ],
    "use_cases": [
        "Customer segmentation",
        "Anomaly detection",
        "Market basket analysis",
        "Dimensionality reduction"
    ]
}

REINFORCEMENT_LEARNING = {
    "definition": "Learning through interaction with environment",
    "key_concepts": [
        "Agent and Environment",
        "States, Actions, and Rewards",
        "Policy and Value Functions",
        "Exploration vs Exploitation"
    ],
    "algorithms": [
        "Q-Learning",
        "Deep Q-Networks (DQN)",
        "Policy Gradient Methods",
        "Actor-Critic Methods"
    ],
    "applications": [
        "Game playing (Chess, Go)",
        "Autonomous vehicles",
        "Robot control",
        "Trading strategies"
    ]
}

# Key ML Concepts

def explain_bias_variance_tradeoff():
    """
    Bias-Variance Tradeoff:
    - High Bias: Model is too simple, underfits the data
    - High Variance: Model is too complex, overfits the data
    - Goal: Find the sweet spot that minimizes total error
    """
    pass

def explain_cross_validation():
    """
    Cross-Validation:
    - Technique to assess model performance and generalization
    - K-fold CV: Split data into k folds, train on k-1, test on 1
    - Helps detect overfitting and select hyperparameters
    - Common values: 5-fold or 10-fold cross-validation
    """
    pass

def explain_feature_engineering():
    """
    Feature Engineering:
    - Process of selecting and transforming variables for ML models
    - Techniques: normalization, encoding categorical variables, 
      creating interaction terms, polynomial features
    - Often more important than algorithm choice for performance
    - Domain knowledge is crucial for effective feature engineering
    """
    pass

# Popular ML Libraries and Frameworks

PYTHON_ML_ECOSYSTEM = {
    "scikit-learn": "General-purpose ML library with many algorithms",
    "pandas": "Data manipulation and analysis",
    "numpy": "Numerical computing with arrays",
    "matplotlib/seaborn": "Data visualization",
    "tensorflow": "Deep learning framework by Google",
    "pytorch": "Deep learning framework by Meta",
    "xgboost": "Gradient boosting framework",
    "lightgbm": "Fast gradient boosting by Microsoft"
}

# Model Evaluation Metrics

CLASSIFICATION_METRICS = [
    "Accuracy: (TP + TN) / (TP + TN + FP + FN)",
    "Precision: TP / (TP + FP)",
    "Recall (Sensitivity): TP / (TP + FN)",
    "F1-Score: 2 * (Precision * Recall) / (Precision + Recall)",
    "ROC-AUC: Area under the ROC curve",
    "Confusion Matrix: Table showing actual vs predicted classifications"
]

REGRESSION_METRICS = [
    "Mean Absolute Error (MAE): Average of absolute differences",
    "Mean Squared Error (MSE): Average of squared differences", 
    "Root Mean Squared Error (RMSE): Square root of MSE",
    "R-squared: Proportion of variance explained by the model",
    "Mean Absolute Percentage Error (MAPE): Percentage-based error metric"
]

# Best Practices for ML Projects

ML_BEST_PRACTICES = [
    "Start with simple models before trying complex ones",
    "Always split data into train/validation/test sets",
    "Use cross-validation for model selection",
    "Monitor for data leakage and overfitting",
    "Document your experiments and results",
    "Consider ethical implications and bias in your models",
    "Plan for model deployment and monitoring in production",
    "Continuously retrain models as new data becomes available"
]